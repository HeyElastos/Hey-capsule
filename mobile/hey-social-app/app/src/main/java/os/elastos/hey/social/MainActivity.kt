package os.elastos.hey.social

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.draw.shadow
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.outlined.FavoriteBorder
import androidx.compose.material.icons.outlined.ChatBubbleOutline
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import com.google.zxing.BarcodeFormat
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.ByteArrayOutputStream

// Gold accent + dark-on-gold are constant across themes.
private val Gold = Color(0xFFD4B84B)
private val Gold2 = Color(0xFFFACC15)
private val Like = Color(0xFFFF5A7A)
private val Navy = Color(0xFF091427) // dark text/icon that sits ON gold (both themes)

// Theme: dark (navy) ↔ light (silver-white + gold). Snapshot state so toggling
// recomposes the whole UI; persisted in prefs (set in onCreate).
private var heyLight by mutableStateOf(false)

private val bg1: Color get() = if (heyLight) Color(0xFFF6F7FB) else Color(0xFF0B1A36)
private val bg2: Color get() = if (heyLight) Color(0xFFEDEFF5) else Color(0xFF071021)
private val bg3: Color get() = if (heyLight) Color(0xFFDFE4EE) else Color(0xFF040A14)
private val ink: Color get() = if (heyLight) Color(0xFF13213B) else Color(0xFFEAF0FA)
private val muted: Color get() = if (heyLight) Color(0xFF5B6B86) else Color(0xFF8DA0BE)
private val glassFill: Color get() = if (heyLight) Color(0x0F0B1A36) else Color(0x0EFFFFFF)
private val glassBorder: Color get() = if (heyLight) Color(0x1F0B1A36) else Color(0x1AFFFFFF)
private val sheetBg: Color get() = if (heyLight) Color(0xFFFFFFFF) else Color(0xFF0C1A33)
// Gold as TEXT/thin-icon on light bg fails contrast — use a deeper gold there.
private val goldInk: Color get() = if (heyLight) Color(0xFF8A6D12) else Gold
// "good"/online green, readable as text on both backgrounds.
private val good: Color get() = if (heyLight) Color(0xFF1E9E54) else Color(0xFF78E68C)
// Incoming chat bubble fill (was white-on-white in light mode).
private val bubbleIn: Color get() = if (heyLight) Color(0xFFFFFFFF) else Color(0x14FFFFFF)

// GodotHost: MainActivity hosts the Hey Verse engine (a GodotFragment in the
// Verse dock tab). The engine boots from assets/verse.pck; ONE instance per
// process ever, so the fragment is created once and kept alive (see VerseHost).
class MainActivity : androidx.fragment.app.FragmentActivity(), org.godotengine.godot.GodotHost {
    companion object {
        /** True while the Verse tab is open. The embedded Godot engine applies
         *  the project's orientation to the HOST activity during boot — this
         *  flag lets us veto that and keep Verse portrait no matter what. */
        @JvmStatic @Volatile var verseOpen = false

        /** The Godot engine allows ONE instance per process, ever. Once a
         *  GodotFragment has existed, a future activity can never re-host it. */
        @JvmStatic @Volatile var verseEverCreated = false
    }

    override fun onDestroy() {
        super.onDestroy()
        // Closing the app (back/swipe) destroys this activity but the foreground
        // service keeps the PROCESS — and the un-rehostable engine — alive, so
        // reopening showed a stuck/black verse. End the process here instead:
        // START_STICKY + the alarm + boot receiver bring the service back in the
        // background within seconds (delivery continues), and the next open gets
        // a completely fresh engine. Never stuck again.
        if (verseEverCreated && !isChangingConfigurations) {
            android.os.Process.killProcess(android.os.Process.myPid())
        }
    }

    override fun setRequestedOrientation(requestedOrientation: Int) {
        super.setRequestedOrientation(
            if (verseOpen) android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
            else requestedOrientation
        )
    }

    override fun getActivity(): android.app.Activity = this
    override fun getGodot(): org.godotengine.godot.Godot? =
        (supportFragmentManager.fragments.firstOrNull { it is org.godotengine.godot.GodotFragment }
            as? org.godotengine.godot.GodotFragment)?.godot
    override fun getCommandLine(): MutableList<String> = mutableListOf("--main-pack", "res://verse.pck")
    // Register the Verse<->Hey bridge: GDScript reaches it via
    // Engine.get_singleton("HeyVerse") — real DID, contacts, invites, presence.
    override fun getHostPlugins(engine: org.godotengine.godot.Godot): MutableSet<org.godotengine.godot.plugin.GodotPlugin> =
        mutableSetOf(HeyVersePlugin(engine))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        heyLight = getSharedPreferences("hey", android.content.Context.MODE_PRIVATE).getBoolean("light", false)
        // Hey's own push: keep the in-process carrier alive in a foreground service
        // and surface peer events as LOCAL notifications (no Firebase → GrapheneOS-OK).
        if (Build.VERSION.SDK_INT >= 33) {
            val ask = registerForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) {}
            if (checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
                ask.launch(android.Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        RuntimeService.start(applicationContext)
        HeyApi.clearDecryptedCache(applicationContext) // E2E media must not linger in cache
        androidx.core.view.WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = android.graphics.Color.TRANSPARENT
        window.navigationBarColor = android.graphics.Color.TRANSPARENT
        setContent {
            val view = androidx.compose.ui.platform.LocalView.current
            SideEffect {
                val c = androidx.core.view.WindowCompat.getInsetsController(window, view)
                c.isAppearanceLightStatusBars = heyLight
                c.isAppearanceLightNavigationBars = heyLight
            }
            val scheme = if (heyLight) {
                lightColorScheme(primary = Gold, background = bg2, surface = bg2, onPrimary = Navy, onBackground = ink, onSurface = ink)
            } else {
                darkColorScheme(primary = Gold, background = bg2, surface = bg2, onPrimary = Navy, onBackground = ink, onSurface = ink)
            }
            MaterialTheme(colorScheme = scheme) { HeyApp() }
        }
        handleCallIntent(intent)
        handleFollowIntent(intent)
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleCallIntent(intent)
        handleFollowIntent(intent)
    }

    /** Handle a `hey:follow:` / `hey-invite:` deep link (a tapped Hey link, or
     *  `adb shell am start -d`). We follow/accept in the background so the peer
     *  receives OUR keys and a DM contact is bootstrapped on both sides — no
     *  camera scan needed. */
    private fun handleFollowIntent(intent: android.content.Intent?) {
        val data = intent?.dataString ?: return
        if (!(data.startsWith("hey:follow:") || data.startsWith("hey-invite:"))) return
        lifecycleScope.launch(Dispatchers.IO) {
            runCatching {
                val res = if (data.startsWith("hey-invite:")) HeyApi.acceptInvite(data) else HeyApi.follow(data)
                val did = res.optString("did")
                if (did.isNotEmpty()) HeyApi.startChat(did)
            }
        }
    }

    /** Show the call UI OVER the lock screen + wake the screen for an incoming call, and
     *  accept directly when launched from the notification's Answer action. The flag is
     *  cleared again when the call ends (see the callState effect in HeyApp) so Hey never
     *  shows over the lock screen outside a call. */
    private fun handleCallIntent(intent: android.content.Intent?) {
        if (intent == null) return
        val answer = intent.getBooleanExtra(CallNotifier.EXTRA_ANSWER_CALL, false)
        val incoming = intent.getBooleanExtra(CallNotifier.EXTRA_INCOMING_CALL, false)
        if (answer) runCatching { CallManager.accept() }
        if (answer || incoming || CallManager.state is CallManager.State.Incoming) {
            if (Build.VERSION.SDK_INT >= 27) {
                runCatching { setShowWhenLocked(true) }
                runCatching { setTurnScreenOn(true) }
            } else {
                @Suppress("DEPRECATION")
                window.addFlags(
                    android.view.WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                        android.view.WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON,
                )
            }
        }
    }
}

// ── frosted-glass helpers ────────────────────────────────────────────────────

/** Translucent frosted panel over the gradient scene. */
private fun Modifier.glass(radius: Dp = 18.dp): Modifier = this
    .clip(RoundedCornerShape(radius))
    .background(glassFill)
    .border(1.dp, glassBorder, RoundedCornerShape(radius))

@Composable
private fun FrostBackground(content: @Composable BoxScope.() -> Unit) {
    val light = heyLight
    val grad = remember(light) { Brush.verticalGradient(listOf(bg1, bg2, bg3)) }
    Box(Modifier.fillMaxSize().background(grad)) {
        // Soft drifting glow blobs, blurred → the "floating scene" behind glass.
        // Light mode uses pale, brighter washes so they read as glow, not grime.
        Canvas(Modifier.fillMaxSize().blur(90.dp)) {
            val (c1, c2, c3) = if (light) {
                Triple(Gold2.copy(alpha = 0.12f), Color(0xFF8FB8E0).copy(alpha = 0.22f), Color(0xFFB6A6E8).copy(alpha = 0.12f))
            } else {
                Triple(Gold.copy(alpha = 0.16f), Color(0xFF2A6FB0).copy(alpha = 0.20f), Color(0xFF7A4FD0).copy(alpha = 0.12f))
            }
            drawCircle(c1, size.minDimension * 0.38f, Offset(size.width * 0.18f, size.height * 0.12f))
            drawCircle(c2, size.minDimension * 0.42f, Offset(size.width * 0.88f, size.height * 0.42f))
            drawCircle(c3, size.minDimension * 0.34f, Offset(size.width * 0.5f, size.height * 0.92f))
        }
        content()
    }
}

private enum class Tab { Chat, Feed, Verse, Activity, Wallet, Profile }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HeyApp() {
    var ready by remember { mutableStateOf(false) }
    var did by remember { mutableStateOf("") }
    var online by remember { mutableStateOf(false) }
    var peers by remember { mutableStateOf(0) }
    var showRelayNotice by remember { mutableStateOf(false) }
    var tab by remember { mutableStateOf(Tab.Chat) }
    var verseSheet by remember { mutableStateOf<String?>(null) }
    var verseExiting by remember { mutableStateOf(false) }
    // Placing a catalog object: the Library sheet started it ("place:<id>"); a
    // small non-modal bar lets you drag it in the world + Rotate/Place/Cancel.
    var versePlacing by remember { mutableStateOf<String?>(null) }
    // Moving an ALREADY-placed object (from its info sheet's "Move"): drag + Rotate/Done.
    var verseEditing by remember { mutableStateOf(false) }
    // The Verse asks for the placement bar via openSheet("placing:<id>") once a NEW
    // ghost is spawned; turn that into the non-modal bar (not a modal sheet).
    LaunchedEffect(verseSheet) {
        val s = verseSheet
        if (s != null && s.startsWith("placing:")) {
            versePlacing = s.removePrefix("placing:")
            verseSheet = null
        }
    }
    var feedVersion by remember { mutableStateOf(0) }
    var feedRev by remember { mutableStateOf(0L) }
    var composing by remember { mutableStateOf(false) }
    var unread by remember { mutableStateOf(0) }
    var profileDid by remember { mutableStateOf<String?>(null) }
    var openChatDid by remember { mutableStateOf<String?>(null) }
    var openConversation by remember { mutableStateOf<Chat?>(null) }
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE) }
    var welcomed by remember { mutableStateOf(prefs.getBoolean("welcomed", false)) }
    // Notifications: a top-right bell + popup (replaces the Activity dock tab). Badge = new activity
    // since last opened; opening marks it seen so old ones clear.
    var showNotifs by remember { mutableStateOf(false) }
    var notifCount by remember { mutableStateOf(0) }
    var notifSeen by remember { mutableStateOf(prefs.getInt("notif_seen", 0)) }
    val activity = ctx as? androidx.fragment.app.FragmentActivity
    val scope = rememberCoroutineScope()
    // Hey Verse is portrait: the embedded Godot engine cannot set the HOST
    // activity's orientation (that only works in standalone exports), so pin
    // portrait while the Verse tab is open and restore sensor elsewhere.
    LaunchedEffect(tab) {
        MainActivity.verseOpen = tab == Tab.Verse
        activity?.requestedOrientation = if (tab == Tab.Verse)
            android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
        else
            android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        // Verse is a game: keep the screen awake while its tab is open —
        // otherwise the display times out and the GL surface comes back black.
        activity?.window?.let { w ->
            if (tab == Tab.Verse)
                w.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            else
                w.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }
    // Vault: if the seed is sealed in the keystore, gate startup on a biometric
    // unlock (the runtime can't start without the seed → background pauses while
    // locked). Otherwise we start immediately (plaintext seed, vault off).
    var unlocked by remember { mutableStateOf(!(IdentityVault.isOn(ctx) && IdentityVault.hasSealed(ctx))) }
    // First run (brand new): no identity on disk, nothing sealed, not welcomed → let
    // the user CHOOSE create-new vs restore BEFORE the runtime auto-creates a fresh
    // identity (otherwise restore would be impossible).
    var needChoice by remember { mutableStateOf(!welcomed && !HeyApi.hasIdentity(ctx) && !(IdentityVault.isOn(ctx) && IdentityVault.hasSealed(ctx))) }
    var restoring by remember { mutableStateOf(false) }
    // The first-run generation animation drives this true when it FINISHES, so the elegant
    // steps always play fully (local key-gen is near-instant, so gating the screen on `ready`
    // alone made it flash by / look skipped).
    var genDone by remember { mutableStateOf(false) }
    // One-time prompt to move an existing PLAINTEXT seed into the hardware vault (restore + users who
    // onboarded before vault-on-by-default). New accounts seal during onboarding, so they never hit this.
    var offerVault by remember { mutableStateOf(false) }
    // H2.2 migration gate: the phrase the user must record before the proactive seal.
    var migrateGatePhrase by remember { mutableStateOf<String?>(null) }
    // H2.6: surfaced when a legacy mnemonic-less account can't be hardware-sealed.
    var legacyNoPhrase by remember { mutableStateOf(false) }
    // H2.5: re-offer migration ONCE PER COLD START for capable devices (no longer a
    // PERMANENT vault_offered suppression) — a single "Not now" must not strand the seed
    // under the no-auth DEK forever. This in-memory flag resets every process launch.
    var offeredThisLaunch by remember { mutableStateOf(false) }
    LaunchedEffect(ready, welcomed) {
        // Only AFTER onboarding (`welcomed`): `ready` flips true while the profile
        // screen is still up, and the vault state read at that moment predates the
        // user's onboarding choice — offering then meant a SECOND fingerprint
        // prompt right after the onboarding seal (the one seal already covers the
        // DID and every wallet, since they all derive from the single seed).
        //
        // Priority 1: do NOT auto-nag users who completed the new onboarding chooser —
        // onboarding sets `vault_offered=true`, so a fresh user who picked "Open freely"
        // is never prompted to enable the vault (they opt in later via Settings → App
        // lock). Only LEGACY pre-change installs (vault_offered unset) still see the
        // one-per-cold-start migration offer, so plaintext-seed accounts can still seal.
        if (ready && welcomed && IdentityVault.available(ctx) &&
            !IdentityVault.isOn(ctx) && !IdentityVault.hasSealed(ctx) &&
            HeyApi.hasIdentity(ctx) && !offeredThisLaunch &&
            !prefs.getBoolean("vault_offered", false)) {
            offeredThisLaunch = true
            offerVault = true
        }
    }
    // Auto re-lock: after the app has been in the background a while, re-gate the UI behind a fresh
    // fingerprint/PIN. The seed STAYS in memory (messages keep arriving in the background — Option A);
    // only the screen re-locks, so returning needs a fresh unlock. No-op when the vault is off.
    var bgAt by remember { mutableStateOf(0L) }
    DisposableEffect(activity) {
        val a = activity ?: return@DisposableEffect onDispose { }
        val obs = androidx.lifecycle.LifecycleEventObserver { _, e ->
            when (e) {
                androidx.lifecycle.Lifecycle.Event.ON_STOP -> {
                    RuntimeService.appForeground = false // re-enable notifications once backgrounded
                    bgAt = System.currentTimeMillis()
                    // Block the OS recents thumbnail at the window level (same primitive as
                    // SecureWindow) so DM/feed/Verse content never leaks into the snapshot —
                    // WindowManager-level, so no recompose/render race or SurfaceView last-
                    // buffer leak. Cleared on ON_START. No DEK/storage change (Option A intact).
                    runCatching { a.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE) }
                    // Option A (user-chosen): KEEP the DEK in memory while locked so the
                    // runtime keeps receiving + DECRYPTING in the background — incoming
                    // CALLS RING and DMs arrive with content while the phone is locked,
                    // like a normal phone. Privacy is preserved by the UI re-gate below
                    // (a fresh fingerprint/PIN is still required to OPEN the app after the
                    // grace window). The key lives ONLY in RAM (never written plaintext)
                    // and is gone on reboot/process-death, so the at-rest vault stays
                    // sealed. (We deliberately no longer call HeyApi.lockStorage() here.)
                }
                androidx.lifecycle.Lifecycle.Event.ON_START -> {
                    RuntimeService.appForeground = true // in-app → suppress event notifications
                    runCatching { a.window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE) }
                    // Priority 3: re-arm the storage DEK on EVERY resume, idempotently, BEFORE
                    // anything sends/flushes — so storage_locked()/processing_deferred() can't
                    // stay true and strand the outbox (the new-conversation PQ-invite + first DM
                    // regression). Cheap + idempotent: in Open-freely mode the DEK never left RAM
                    // (no-op re-install); in require-unlock mode the quick-return path needs it.
                    // The long-background re-lock path below routes through the biometric unseal,
                    // which re-installs the DEK before unlock — but we still re-arm here for the
                    // common (vault-off / quick-return) cases.
                    if (HeyApi.isStarted) {
                        scope.launch { withContext(Dispatchers.IO) { HeyApi.installStorageKey(ctx) } }
                    }
                    if (IdentityVault.isOn(ctx) && IdentityVault.hasSealed(ctx) && bgAt > 0L) {
                        if (System.currentTimeMillis() - bgAt > 120_000L) {
                            // Option A keeps the DEK in RAM across the lock, so a spend grant
                            // minted before backgrounding would still be redeemable. Kill them
                            // here: re-gating the UI must invalidate pending money authorizations.
                            HeyApi.revokeSpends()
                            unlocked = false // long background -> biometric (re-installs DEK on unlock)
                        }
                    }
                }
                else -> {}
            }
        }
        a.lifecycle.addObserver(obs)
        onDispose { a.lifecycle.removeObserver(obs) }
    }

    LaunchedEffect(unlocked, needChoice) {
        if (!unlocked || needChoice) return@LaunchedEffect // wait for the new/restore choice
        withContext(Dispatchers.IO) {
            runCatching {
                HeyApi.ensureStarted(ctx, HeyApi.unlockedSeed) // unlockedSeed = restore phrase, or null = new
                did = HeyApi.whoami().optString("did")
            }
        }
        ready = true
    }
    // A restored account skips profile setup (their profile syncs from the network).
    LaunchedEffect(ready, restoring, genDone) {
        if (ready && restoring && genDone && !welcomed) { prefs.edit().putBoolean("welcomed", true).apply(); welcomed = true }
    }
    // Provision the wallet for EVERY user the moment the runtime is ready: derive +
    // publish receive addresses (so others can tip you) and mark the wallet set up,
    // so the Wallet tab opens ready and tips resolve from the start. Backfills users
    // who onboarded before this existed.
    LaunchedEffect(ready) {
        if (ready && !prefs.getBoolean("tips_published", false)) {
            withContext(Dispatchers.IO) { runCatching { HeyApi.provisionWallet(ctx) } }
        }
    }
    LaunchedEffect(ready) {
        if (!ready) return@LaunchedEffect
        while (true) {
            withContext(Dispatchers.IO) {
                runCatching {
                    val h = HeyApi.health(); online = h.optBoolean("online"); peers = h.optInt("peer_count")
                    // One-time transparency notice if we're connected but only via the relay (not direct).
                    if (online && !h.optBoolean("direct") && !prefs.getBoolean("relay_notice_seen", false)) {
                        showRelayNotice = true
                    }
                    unread = HeyApi.hey_total_unread()
                    notifCount = HeyApi.followers().size
                }
            }
            kotlinx.coroutines.delay(3000)
        }
    }
    // Auto-refresh: the receiver bumps feed_rev when it ingests carrier events;
    // we poll that cheap counter and reload the UI only when it changes.
    LaunchedEffect(ready) {
        if (!ready) return@LaunchedEffect
        while (true) {
            val r = withContext(Dispatchers.IO) { runCatching { HeyApi.hey_feed_rev() }.getOrDefault(feedRev) }
            if (r != feedRev) feedRev = r
            kotlinx.coroutines.delay(1500)
        }
    }

    // Always-on BEAM sync while the app is open (keeps the balance fresh + confirms sends).
    // Quicksync only (a mobile/own node self-syncs); gated on unlocked (seed available) so it
    // never spins on a locked vault. beamSyncStart is idempotent (guarded). NOTE: quick-sync vs a
    // PUBLIC node refreshes KNOWN coins + send-state but CANNOT discover incoming public-offline
    // payments — BEAM reveals those only to an OWNED node (see the BEAM receive notes).
    LaunchedEffect(ready) {
        if (!ready) return@LaunchedEffect
        while (true) {
            if (BeamApi.available && HeyApi.beamNodeMode(ctx) == "quicksync" && !HeyApi.processingDeferred()) {
                withContext(Dispatchers.IO) { runCatching { HeyApi.beamSyncStart(ctx) } }
            }
            kotlinx.coroutines.delay(180_000L) // ~3 min
        }
    }

    // Drive 1:1 voice-call signaling while the app is open + the runtime is up.
    LaunchedEffect(ready) { if (ready) CallManager.startPolling() }
    // Hey Verse lane: app-wide drain — invites pop up ANYWHERE in Hey, like
    // an incoming call, even when the Verse tab is closed.
    var verseInvite by remember { mutableStateOf<Triple<String, String, String>?>(null) }
    LaunchedEffect(ready) {
        if (ready) {
            HeyVersePlugin.startLane()
            // Priority 2: re-arm the in-process spend binding if the user left it ON (reads
            // only the PUBLIC key — NO biometric). We DO NOT auto-enroll at boot anymore:
            // opening the app must trigger ZERO spend prompts. The P-256 spend key is enrolled
            // LAZILY at the FIRST wallet send (inside the spend-confirm flow), so the first
            // send's single biometric both enrolls and authorizes (SpendAuth.spendGrant).
            runCatching { if (SpendAuth.isEnrolled(ctx)) SpendAuth.reenroll(ctx) }
            while (true) {
                verseInvite = HeyVersePlugin.pendingInvite
                kotlinx.coroutines.delay(400)
            }
        }
    }

    // True once the app has fully booted into the main UI at least once this
    // session (all the boot gates below have passed). Drives the re-lock path:
    // before first boot the lock is a full-screen early-return (nothing is
    // mounted, the runtime isn't up); AFTER boot a re-lock must NOT unmount the
    // content (the GodotFragment can be hosted only ONCE per process — see the
    // single-host comment near MainActivity), so the lock is drawn as an opaque
    // overlay ON TOP of the still-mounted content instead.
    var everBooted by remember { mutableStateOf(false) }
    FrostBackground {
        // Vault gate FIRST (before the runtime even starts): unlock → unseal seed
        // → that seed boots the runtime. Until then the carrier stays down.
        //
        // The lock UI + its biometric/unlock callback are defined ONCE here and
        // reused for both presentations (boot early-return AND post-boot overlay)
        // so the headless-vault unseal → installStorageKey → unlock flow and the
        // seed==null / -2 → restore routes are never duplicated or weakened.
        val lockUi: @Composable () -> Unit = {
            // Routes the user to the create/restore flow when biometric unlock can't
            // recover the seed (lock removed → key invalidated, wrong account, etc.).
            // The LaunchedEffect(unlocked, needChoice) gate keeps the runtime from
            // starting with no/wrong seed while needChoice is true.
            val toRestore: (String?) -> Unit = { msg ->
                if (msg != null) android.widget.Toast.makeText(ctx, msg, android.widget.Toast.LENGTH_LONG).show()
                // Abandoning the current identity (lock changed / wrong account / restore):
                // no pending money authorization from the old session may carry over.
                HeyApi.revokeSpends()
                HeyApi.unlockedSeed = null
                needChoice = true
                unlocked = true
            }
            LockScreen(onRestore = { toRestore(null) }) {
                activity?.let { a ->
                    // H2.1: the unseal is bound to a fresh BiometricPrompt CryptoObject
                    // (the seed decrypts ONLY inside this prompt's success callback), so
                    // there's no 30s-stale-auth window and no separate AppLock prompt.
                    IdentityVault.unsealAuthed(a) { unsealed, deadKey ->
                        if (unsealed == null) {
                            if (deadKey) scope.launch {
                                // The hardware key is permanently invalidated — the device
                                // lock was changed/removed (or the seal is corrupt). Clear
                                // the dead seal and route to restore (the seed is recoverable
                                // from the recovery phrase).
                                withContext(Dispatchers.IO) { IdentityVault.clear(ctx) }
                                toRestore("Your device lock changed, so the on-device key was reset. Restore from your recovery phrase to continue.")
                            }
                            // else: user cancelled — stay locked (no state change).
                            return@unsealAuthed
                        }
                        scope.launch {
                            val seed = unsealed
                            HeyApi.unlockedSeed = seed
                            val rc = withContext(Dispatchers.IO) {
                                // Re-install the DEK cleared on lock (idempotent if the
                                // headless boot already has it).
                                HeyApi.installStorageKey(ctx)
                                // Old-vaulted device with no carrier blob: the headless
                                // boot refused, so start Full now (this also backfills the
                                // blob). Headless-booted device: already started → no-op.
                                HeyApi.ensureStarted(ctx, seed)
                                // Hand the runtime the unsealed seed: a headless boot now
                                // installs IDENTITY and drains buffered messages; a Full
                                // boot above already set it, so this returns 0 (idempotent).
                                HeyApi.unlock(seed)
                            }
                            if (rc == -2) {
                                // Carrier node key ≠ this seed. Unreachable in supported
                                // flows (the blob is written together with the seal); never
                                // mesh as the wrong account. Clear the stale wrong-account
                                // seal (mirroring the seed==null branch) BEFORE routing, so
                                // the restored account boots cleanly and the user isn't
                                // re-locked into this branch every launch.
                                withContext(Dispatchers.IO) { IdentityVault.clear(ctx) }
                                toRestore("This device's identity doesn't match. Restore from your recovery phrase.")
                            } else {
                                unlocked = true
                            }
                        }
                    }
                }
            }
        }
        // BOOT lock only: nothing is mounted yet and the runtime isn't up, so a
        // full-screen early-return is correct. A RE-LOCK after boot is handled
        // further down as an opaque overlay so the GodotFragment stays alive.
        if (!unlocked && !everBooted) {
            lockUi()
            return@FrostBackground
        }
        // First-run: swipeable welcome + the create-new / restore-phrase choice,
        // BEFORE the runtime starts (so a restore can supply the seed).
        if (needChoice) {
            WelcomeFlow(
                onCreateNew = { needChoice = false },
                onRestore = { phrase -> HeyApi.unlockedSeed = phrase; restoring = true; needChoice = false },
            )
            return@FrostBackground
        }
        // Fresh setup: ALWAYS play the elegant generation/restore animation to completion
        // (onDone sets genDone). Gating on the animation finishing — not on `ready` — keeps it
        // from flashing by, since local key-gen finishes in well under a second.
        if (!welcomed && !genDone) {
            GeneratingSteps(
                title = if (restoring) "Restoring your account" else "Creating your Hey identity",
                subtitle = if (restoring) "Re-deriving your keys, DID and wallets from your recovery phrase."
                else "Your keys are generated and held only on this phone.",
                steps = listOf(
                    "Deriving your keys" to Icons.Filled.Key,
                    (if (restoring) "Restoring your Hey identity" else "Building your Hey identity") to Icons.Filled.Fingerprint,
                    "Setting up your wallets (ELA · ESC · EID)" to Icons.Filled.AccountBalanceWallet,
                    "Securing on this device" to Icons.Filled.Shield,
                ),
                onDone = { genDone = true },
            )
            return@FrostBackground
        }
        if (!ready) {
            // Runtime still coming up (rare — the animation usually outlasts it) → minimal spinner.
            Column(Modifier.fillMaxSize(), Arrangement.Center, Alignment.CenterHorizontally) {
                Text("Hey", color = goldInk, fontSize = 52.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(16.dp))
                CircularProgressIndicator(color = goldInk)
                Spacer(Modifier.height(14.dp))
                Text("Starting your on-device runtime…", color = muted)
            }
            return@FrostBackground
        }
        if (!welcomed) {
            OnboardingScreen(did) {
                // Onboarding IS the vault decision: its one fingerprint seals the
                // single seed behind the DID and every wallet. Never re-offer.
                prefs.edit().putBoolean("welcomed", true).putBoolean("vault_offered", true).apply()
                welcomed = true
            }
            return@FrostBackground
        }
        // All boot gates have passed — the main UI (incl. the once-per-process
        // GodotFragment host below) is about to mount. From here on a re-lock must
        // keep this content mounted and only overlay the lock screen.
        LaunchedEffect(Unit) { everBooted = true }
        // H2.5: proactive migration offer for existing vault-OFF capable installs.
        // Re-offered once per cold-start (no permanent suppression). H2.2: gated behind
        // RecordPhraseGate before the seal.
        if (offerVault && activity != null) {
            AlertDialog(
                onDismissRequest = { offerVault = false },
                icon = { Icon(Icons.Filled.Fingerprint, null, tint = goldInk) },
                title = { Text("Protect your recovery phrase", color = ink) },
                text = {
                    Text("Lock your seed with this phone's fingerprint or PIN — sealed in the Titan M / Knox Vault / TEE so it's never stored in the clear. You'll unlock Hey with your fingerprint; messages still arrive in the background. We'll show your recovery phrase first so you can record it.",
                        color = muted, fontSize = 13.sp, lineHeight = 19.sp)
                },
                confirmButton = {
                    TextButton(onClick = {
                        offerVault = false
                        scope.launch {
                            // Resolve the phrase to record. H5: when the spend/reveal binding
                            // is enrolled, the bare recoveryPhrase() JNI refuses — use the
                            // signature-verified reveal (same as enableVault).
                            val phrase = HeyApi.unlockedSeed
                                ?: (if (SpendAuth.isEnrolled(ctx)) SpendAuth.revealSeed(activity) else HeyApi.recoveryPhrase())
                            if (phrase.isNullOrBlank()) {
                                // H2.6: legacy mnemonic-less blob — surface the notice (below)
                                // instead of silently sealing nothing.
                                legacyNoPhrase = true
                            } else {
                                migrateGatePhrase = phrase
                            }
                        }
                    }) { Text("Enable", color = goldInk, fontWeight = FontWeight.Bold) }
                },
                dismissButton = {
                    TextButton(onClick = { offerVault = false }) { Text("Not now", color = muted) }
                },
                containerColor = sheetBg,
            )
        }
        // H2.2: the migration phrase-record gate. On confirm → enableVault (atomic seal).
        migrateGatePhrase?.let { phrase ->
            if (activity != null) RecordPhraseGate(
                phrase = phrase,
                onConfirmed = {
                    migrateGatePhrase = null
                    enableVault(activity, ctx, scope) { ok ->
                        android.widget.Toast.makeText(ctx, if (ok) "Keys sealed in hardware" else "Couldn't enable — try again", android.widget.Toast.LENGTH_SHORT).show()
                    }
                },
                onCancel = { migrateGatePhrase = null }, // not sealed; re-offered next cold start
            )
        }
        // H2.6: one-time notice for legacy seed-only (mnemonic-less) accounts that can't
        // be hardware-sealed. NOT a no-auth seed path — just an honest, explicit signal.
        if (legacyNoPhrase) {
            AlertDialog(
                onDismissRequest = { legacyNoPhrase = false; prefs.edit().putBoolean("legacy_noseal_notified", true).apply() },
                icon = { Icon(Icons.Filled.Info, null, tint = goldInk) },
                title = { Text("Can't hardware-seal this account", color = ink) },
                text = {
                    Text("This account predates recovery phrases, so it has no 12 words to confirm — we can't seal it in the hardware vault. Your data is still sandboxed and encrypted at rest by Android. To get hardware sealing, create a new account (you can keep using this one).",
                        color = muted, fontSize = 13.sp, lineHeight = 19.sp)
                },
                confirmButton = {
                    TextButton(onClick = { legacyNoPhrase = false; prefs.edit().putBoolean("legacy_noseal_notified", true).apply() }) { Text("Got it", color = goldInk, fontWeight = FontWeight.Bold) }
                },
                containerColor = sheetBg,
            )
        }
        // Now that an identity is provisioned, (re)start the foreground service so
        // its notification poll loop runs against the live runtime — this is what
        // keeps peers connected + surfaces DM/mention/tip notifications when the app
        // is closed. Idempotent (the service guards its own polling).
        LaunchedEffect(welcomed) {
            if (welcomed) runCatching { RuntimeService.start(ctx) }
        }
        // Ask to allow always-on background delivery (battery exemption) — the one
        // switch that lets DMs, post mentions and tips notify you with the app
        // closed. Re-asked each launch until granted (or snoozed this session);
        // the Profile tab always has the toggle too.
        var batterySnoozed by remember { mutableStateOf(false) }
        var showBattery by remember { mutableStateOf(false) }
        LaunchedEffect(welcomed, batterySnoozed) {
            if (welcomed && !batterySnoozed && !BatteryHelper.isExempt(ctx)) showBattery = true
        }
        if (showBattery) {
            AlertDialog(
                onDismissRequest = { showBattery = false; batterySnoozed = true },
                icon = { Icon(Icons.Filled.Bolt, null, tint = goldInk) },
                title = { Text("Stay connected in the background", color = ink) },
                text = { Text("Hey delivers everything peer-to-peer — no servers. Allow it to run in the background so you're notified of direct messages, posts that mention you, and tips even when the app is closed. It stays connected using very little battery.", color = muted) },
                confirmButton = {
                    TextButton(onClick = {
                        showBattery = false; batterySnoozed = true
                        BatteryHelper.request(ctx)
                    }) { Text("Allow", color = goldInk, fontWeight = FontWeight.Bold) }
                },
                dismissButton = {
                    TextButton(onClick = { showBattery = false; batterySnoozed = true }) { Text("Not now", color = muted) }
                },
                containerColor = sheetBg,
            )
        }
        // System back: cancel the composer, else return to the Chat tab, before
        // ever exiting the app.
        androidx.activity.compose.BackHandler(enabled = composing) { composing = false }
        androidx.activity.compose.BackHandler(enabled = !composing && tab != Tab.Chat) { tab = Tab.Chat }
        Scaffold(
            containerColor = Color.Transparent,
            contentWindowInsets = androidx.compose.foundation.layout.WindowInsets(0, 0, 0, 0),
        ) { pad ->
            // Content fills the full height and scrolls BEHIND the floating top bar (true frosted
            // glass — the bar reveals the moving content, not just the static backdrop). Each tab's
            // scroll gets `topPad` of head-room so its first item clears the bar.
            val barInset = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
            val topPad = barInset + 80.dp
            // Blur the whole app behind the notifications popup (frosted backdrop, API 31+).
            val bgBlur by animateDpAsState(if (showNotifs) 16.dp else 0.dp, tween(220), label = "bgBlur")
            Box(Modifier.padding(pad).fillMaxSize().blur(bgBlur)) {
                // Fade content out where it meets the top bar + the bottom dock.
                Box(Modifier.fillMaxSize().fadeEdges(barInset + 72.dp, 72.dp)) {
                // Hey Verse host: the Godot engine supports ONE instance per process,
                // ever — created lazily on the first visit, then KEPT alive as a
                // STATIC fullscreen surface (moving it off-screen starved the
                // surface → black screen). Other tabs draw opaque content over it.
                var verseOpened by remember { mutableStateOf(false) }
                var verseReady by remember { mutableStateOf(false) }
                LaunchedEffect(tab) {
                    if (BuildConfig.VERSE_ENABLED && tab == Tab.Verse) {
                        verseOpened = true
                        HeyVersePlugin.postUi("wake")
                        // the game can ask for app sheets (tap Sash → his FAQ)
                        while (true) {
                            HeyVersePlugin.takeSheetRequest()?.let { verseSheet = it }
                            kotlinx.coroutines.delay(250)
                        }
                    }
                }
                LaunchedEffect(verseOpened) {
                    while (verseOpened && !verseReady) {
                        verseReady = HeyVersePlugin.gameReadyFlag
                        kotlinx.coroutines.delay(250)
                    }
                }
                var godotFrag by remember { mutableStateOf<org.godotengine.godot.GodotFragment?>(null) }
                if (verseOpened) {
                    androidx.fragment.compose.AndroidFragment<org.godotengine.godot.GodotFragment>(
                        modifier = Modifier.fillMaxSize()
                    ) { f -> godotFrag = f; MainActivity.verseEverCreated = true }
                    // Off the Verse tab the surface is fully DETACHED (GONE):
                    // zero GPU work, and no hole-punch bleed through frosted or
                    // alpha-faded chrome (the "verse peeking at the bottom of
                    // Feed/Chat" bug). onUpdate alone doesn't re-run per tab
                    // switch, so visibility is driven from an effect instead.
                    LaunchedEffect(tab, godotFrag) {
                        godotFrag?.view?.visibility =
                            if (tab == Tab.Verse) android.view.View.VISIBLE else android.view.View.GONE
                    }
                    // BLACK-SCREEN FAILSAFE. Locking the phone can kill the GL context;
                    // when that wedges the embedded engine, no surface trick revives it —
                    // and because the foreground service keeps this PROCESS alive, even
                    // closing and reopening the app shows the same dead engine (only a
                    // reinstall used to clear it). The game polls the plugin every frame,
                    // so a stale heartbeat while we are RESUMED = wedged. Stage 1: bounce
                    // the SurfaceView (heals plain surface loss). Stage 2: cleanly restart
                    // the whole process — Android relaunches us in ~half a second with a
                    // fresh engine, so the verse can NEVER stay black.
                    var resumed by remember { mutableStateOf(true) }
                    DisposableEffect(activity) {
                        val lc = activity?.lifecycle
                        val obs = androidx.lifecycle.LifecycleEventObserver { _, e ->
                            when (e) {
                                androidx.lifecycle.Lifecycle.Event.ON_RESUME -> resumed = true
                                androidx.lifecycle.Lifecycle.Event.ON_PAUSE -> resumed = false
                                else -> {}
                            }
                        }
                        lc?.addObserver(obs)
                        onDispose { lc?.removeObserver(obs) }
                    }
                    LaunchedEffect(tab, verseReady, resumed, godotFrag) {
                        if (tab != Tab.Verse || !verseReady || !resumed) return@LaunchedEffect
                        var bounced = false
                        // settle-in grace so a fresh resume isn't misread as a wedge
                        HeyVersePlugin.lastPollAt = System.currentTimeMillis()
                        while (true) {
                            kotlinx.coroutines.delay(1000)
                            val stale = System.currentTimeMillis() - HeyVersePlugin.lastPollAt
                            if (stale < 3500) { bounced = false; continue }
                            if (!bounced) {
                                godotFrag?.view?.let { v ->
                                    v.visibility = android.view.View.GONE
                                    v.post { v.visibility = android.view.View.VISIBLE }
                                }
                                bounced = true
                                continue
                            }
                            if (stale > 8000) {
                                // stage 2: schedule our own relaunch, then end the process
                                runCatching {
                                    val launch = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)
                                        ?.apply { addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK or android.content.Intent.FLAG_ACTIVITY_CLEAR_TASK) }
                                    val pi = android.app.PendingIntent.getActivity(
                                        ctx, 7741, launch,
                                        android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_CANCEL_CURRENT,
                                    )
                                    val am = ctx.getSystemService(android.content.Context.ALARM_SERVICE) as android.app.AlarmManager
                                    am.set(android.app.AlarmManager.RTC, System.currentTimeMillis() + 450, pi)
                                }
                                android.os.Process.killProcess(android.os.Process.myPid())
                            }
                        }
                    }
                    if (tab != Tab.Verse) {
                        // opaque cover: the surface punches a hole through the window,
                        // so other tabs must fully paint over it
                        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(bg1, bg2, bg3))))
                    } else if (!verseReady) {
                        // boot overlay instead of a black surface
                        Box(
                            Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(bg1, bg2, bg3))),
                            contentAlignment = Alignment.Center
                        ) {
                            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                CircularProgressIndicator(color = Gold, trackColor = glassFill)
                                Spacer(Modifier.height(14.dp))
                                Text("entering your verse…", color = muted, fontSize = 14.sp)
                            }
                        }
                    }
                }
                when (tab) {
                    Tab.Verse -> { if (!BuildConfig.VERSE_ENABLED) VerseComingSoon(topPad) }  // host renders it when enabled
                    Tab.Chat -> ChatListScreen(topPad) { openConversation = it }
                    Tab.Feed -> Box(Modifier.fillMaxSize()) {
                        FeedScreen(feedVersion, feedRev, did, topPad, onOpenProfile = { profileDid = it })
                        FloatingActionButton(
                            onClick = { composing = true },
                            containerColor = Gold, contentColor = Navy,
                            modifier = Modifier.align(Alignment.BottomEnd).padding(end = 20.dp, bottom = 96.dp)
                        ) { Icon(Icons.Filled.Add, "New post") }
                    }
                    Tab.Activity -> NotificationsScreen(topPad, onOpenProfile = { profileDid = it })
                    Tab.Wallet -> WalletScreen(topPad + 14.dp)
                    Tab.Profile -> ProfileScreen(did, online, peers, topPad, onOpenProfile = { profileDid = it })
                }
                }
                // ── Floating glass TOP bar (overlays the scrolling content) ──
                Row(
                    Modifier.align(Alignment.TopCenter).fillMaxWidth()
                        // Frosted-glass panel: a milky, mostly-opaque tint that OBSCURES the content
                        // behind it (so scrolling text reads as softly faded, not see-through), then
                        // dissolves to transparent over a long, many-stop tail so there's no hard
                        // line where the frost ends.
                        .background(
                            Brush.verticalGradient(
                                0.00f to bg2.copy(alpha = 0.94f),
                                0.55f to bg2.copy(alpha = 0.92f),
                                0.70f to bg2.copy(alpha = 0.86f),
                                0.80f to bg2.copy(alpha = 0.72f),
                                0.88f to bg2.copy(alpha = 0.50f),
                                0.94f to bg2.copy(alpha = 0.26f),
                                0.98f to bg2.copy(alpha = 0.10f),
                                1.00f to bg2.copy(alpha = 0f),
                            )
                        )
                        .background(Brush.verticalGradient(listOf(Color.White.copy(alpha = if (heyLight) 0.26f else 0.12f), Color.Transparent)))
                        .statusBarsPadding().padding(18.dp, 14.dp, 18.dp, 26.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("Hey", color = goldInk, fontWeight = FontWeight.Bold, fontSize = 26.sp)
                    Text(when (tab) { Tab.Chat -> " Chat"; Tab.Wallet -> " Wallet"; Tab.Verse -> " Verse"; else -> " Social" }, color = ink, fontWeight = FontWeight.Light, fontSize = 22.sp)
                    Spacer(Modifier.weight(1f))
                    // Notifications bell (replaces the Activity dock tab) + an unseen-activity dot.
                    Box {
                        IconButton(onClick = {
                            showNotifs = true; notifSeen = notifCount
                            prefs.edit().putInt("notif_seen", notifCount).apply()
                        }) { Icon(Icons.Filled.Notifications, "Notifications", tint = goldInk) }
                        if (notifCount > notifSeen) {
                            Box(Modifier.align(Alignment.TopEnd).padding(8.dp).size(9.dp).clip(CircleShape).background(Like))
                        }
                    }
                }
                // Floating frosted dock — in Verse it morphs into the game controls.
                FloatingDock(
                    tab = tab, unread = unread, online = online,
                    onSelect = { t -> tab = t; if (t == Tab.Feed) feedVersion++ },
                    onVerse = { action ->
                        when (action) {
                            "exit" -> {
                                // save now; the goodbye overlay plays, then the
                                // engine truly sleeps and we land back in Chat
                                HeyVersePlugin.postUi("save")
                                verseExiting = true
                            }
                            else -> verseSheet = action
                        }
                    },
                    modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 12.dp)
                )
                // Catalog placement controls — float above the dock while you drag
                // the object in the world (Library "Place" started it). The empty
                // area passes touches through to the game so the drag works.
                if (tab == Tab.Verse && versePlacing != null) {
                    Column(
                        Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 84.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text("drag the object on the floor · snaps to a grid", color = muted, fontSize = 12.sp)
                        Spacer(Modifier.height(6.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Button(
                                onClick = { HeyVersePlugin.postUi("place_rotate") },
                                colors = ButtonDefaults.buttonColors(containerColor = bg2.copy(alpha = 0.96f), contentColor = goldInk),
                                shape = RoundedCornerShape(18.dp),
                            ) { Text("⟲ Rotate", fontSize = 14.sp) }
                            Button(
                                onClick = { HeyVersePlugin.postUi("place_confirm"); versePlacing = null },
                                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                                shape = RoundedCornerShape(18.dp),
                            ) { Text("✓ Place", fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
                            Button(
                                onClick = { HeyVersePlugin.postUi("place_cancel"); versePlacing = null },
                                colors = ButtonDefaults.buttonColors(containerColor = bg2.copy(alpha = 0.96f), contentColor = ink),
                                shape = RoundedCornerShape(18.dp),
                            ) { Text("✕", fontSize = 14.sp) }
                        }
                    }
                }
                // Moving an already-placed object (from its sheet's "Move").
                if (tab == Tab.Verse && verseEditing) {
                    Column(
                        Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 84.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text("drag to move · snaps to a grid", color = muted, fontSize = 12.sp)
                        Spacer(Modifier.height(6.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Button(
                                onClick = { HeyVersePlugin.postUi("place_rotate") },
                                colors = ButtonDefaults.buttonColors(containerColor = bg2.copy(alpha = 0.96f), contentColor = goldInk),
                                shape = RoundedCornerShape(18.dp),
                            ) { Text("⟲ Rotate", fontSize = 14.sp) }
                            Button(
                                onClick = { HeyVersePlugin.postUi("place_cancel"); verseEditing = false },
                                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                                shape = RoundedCornerShape(18.dp),
                            ) { Text("✓ Done", fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
                        }
                    }
                }
            }
        }
        // Hey Verse dock sheets — the game's settings live in Hey's own popup
        // module. The avatar editor is special: almost-fullscreen with a
        // SEE-THROUGH top — the game zooms to a live close-up of your robot
        // and you watch every change land on him as you edit.
        if (verseSheet == "avatar") {
            androidx.activity.compose.BackHandler { HeyVersePlugin.postUi("edit_off"); verseSheet = null }
            VerseAvatarEditor(onClose = { HeyVersePlugin.postUi("edit_off"); verseSheet = null })
        } else if (verseSheet?.startsWith("lift:") == true) {
            // The mall elevator: a native Hey sheet (same module as Sash's chat).
            // The game sent the floor labels; the pick rides back as a ui command.
            val labels = verseSheet!!.removePrefix("lift:").split("|").filter { it.isNotBlank() }
            ModalBottomSheet(
                onDismissRequest = { HeyVersePlugin.postUi("lift_pick:-1"); verseSheet = null },
                containerColor = sheetBg,
            ) {
                Column(Modifier.fillMaxWidth().padding(20.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.SwapVert, null, tint = goldInk, modifier = Modifier.size(22.dp))
                        Spacer(Modifier.width(8.dp))
                        Text("Elevator", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                    }
                    Spacer(Modifier.height(4.dp))
                    Text("where to?", color = muted, fontSize = 12.sp)
                    Spacer(Modifier.height(14.dp))
                    labels.forEachIndexed { i, l ->
                        Button(
                            onClick = { HeyVersePlugin.postUi("lift_pick:$i"); verseSheet = null },
                            modifier = Modifier.fillMaxWidth().height(50.dp),
                            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                        ) { Text(l, fontWeight = FontWeight.Bold, fontSize = 15.sp) }
                        Spacer(Modifier.height(10.dp))
                    }
                    TextButton(
                        onClick = { HeyVersePlugin.postUi("lift_pick:-1"); verseSheet = null },
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text("stay here", color = muted) }
                }
                Spacer(Modifier.height(28.dp))
            }
        } else if (verseSheet?.startsWith("item:") == true) {
            // Tapped a placed object — its info + owner actions (move / remove).
            val id = verseSheet!!.removePrefix("item:")
            val item = (VerseCatalogData.items + VerseBuildingData.items).firstOrNull { it.id == id }
            ModalBottomSheet(
                onDismissRequest = { HeyVersePlugin.postUi("place_cancel"); verseSheet = null },
                containerColor = sheetBg,
            ) {
                Column(Modifier.fillMaxWidth().padding(start = 22.dp, end = 22.dp, bottom = 28.dp)) {
                    Text(item?.name ?: id, color = goldInk, fontSize = 19.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(2.dp))
                    Text((item?.rarity ?: "") + " · " + (item?.kind ?: "object"), color = rarityColor(item?.rarity ?: ""), fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                    Spacer(Modifier.height(8.dp))
                    Text(item?.desc ?: "", color = ink, fontSize = 13.sp, lineHeight = 18.sp)
                    Spacer(Modifier.height(16.dp))
                    if (item?.kind == "seating") {
                        Button(
                            onClick = { HeyVersePlugin.postUi("sit_here"); verseSheet = null },
                            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                            shape = RoundedCornerShape(16.dp), modifier = Modifier.fillMaxWidth(),
                        ) { Text("Sit here", fontWeight = FontWeight.SemiBold) }
                        Spacer(Modifier.height(8.dp))
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(
                            onClick = { HeyVersePlugin.postUi("move_on"); verseSheet = null; verseEditing = true },
                            colors = ButtonDefaults.buttonColors(containerColor = glassFill, contentColor = ink),
                            shape = RoundedCornerShape(16.dp), modifier = Modifier.weight(1f),
                        ) { Text("Move") }
                        Button(
                            onClick = { HeyVersePlugin.postUi("delete_sel"); verseSheet = null },
                            colors = ButtonDefaults.buttonColors(containerColor = glassFill, contentColor = Color(0xFFE07A86)),
                            shape = RoundedCornerShape(16.dp), modifier = Modifier.weight(1f),
                        ) { Text("Remove") }
                    }
                    Spacer(Modifier.height(6.dp))
                    TextButton(
                        onClick = { HeyVersePlugin.postUi("place_cancel"); verseSheet = null },
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text("Done", color = muted) }
                }
            }
        } else if (verseSheet?.startsWith("owned:") == true) {
            // Tried to place a second copy of a 1-of-1 you own.
            val id = verseSheet!!.removePrefix("owned:")
            val item = (VerseCatalogData.items + VerseBuildingData.items).firstOrNull { it.id == id }
            AlertDialog(
                onDismissRequest = { verseSheet = null },
                containerColor = sheetBg,
                title = { Text("Already placed", color = goldInk, fontWeight = FontWeight.Bold) },
                text = { Text("You own one ${item?.name ?: "of these"} and it's already in your world. Collect more to place another.", color = ink, fontSize = 14.sp) },
                confirmButton = { TextButton(onClick = { verseSheet = null }) { Text("OK", color = goldInk) } },
            )
        } else if (verseSheet != null) {
            ModalBottomSheet(onDismissRequest = { verseSheet = null }, containerColor = sheetBg) {
                when (verseSheet) {
                    "worlds" -> VerseWorldsSheet()
                    "invite" -> VerseInviteSheet()
                    "library" -> VerseLibrarySheet(onPlace = { id ->
                        // The Verse decides: a NEW placement opens "placing:<id>"
                        // (→ the bar), an already-owned/placed item opens "item:<id>"
                        // (→ move/remove). Don't assume which here.
                        HeyVersePlugin.postUi("place:$id")
                        verseSheet = null
                    })
                    "sash_faq" -> VerseSashFaqSheet(onClose = { verseSheet = null })
                }
                Spacer(Modifier.height(28.dp))
            }
        }
        // Incoming verse invite — Accept/Decline, shown anywhere in the app.
        verseInvite?.let { inv ->
            AlertDialog(
                onDismissRequest = { HeyVersePlugin.declineInvite(); verseInvite = null },
                containerColor = sheetBg,
                title = { Text("${inv.second} invites you", color = goldInk, fontWeight = FontWeight.Bold) },
                text = {
                    Text(
                        "Join their Verse world${if (inv.third == "city") " — they're in Ela City" else ""}? Live visit: leaving ends the session.",
                        color = ink, fontSize = 14.sp,
                    )
                },
                confirmButton = {
                    Button(
                        onClick = {
                            HeyVersePlugin.acceptInvite()
                            verseInvite = null
                            tab = Tab.Verse
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                        shape = RoundedCornerShape(16.dp),
                    ) { Text("Join") }
                },
                dismissButton = {
                    TextButton(onClick = { HeyVersePlugin.declineInvite(); verseInvite = null }) {
                        Text("Decline", color = muted)
                    }
                },
            )
        }
        // Leaving the verse: a calm gold goodbye while the world saves, then
        // the engine truly sleeps (paused + surface detached) → back to Chat.
        androidx.compose.animation.AnimatedVisibility(
            visible = verseExiting,
            enter = fadeIn(tween(240)), exit = fadeOut(tween(320)),
        ) {
            Box(
                Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(bg1, bg2, bg3))),
                contentAlignment = Alignment.Center
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator(color = Gold, trackColor = glassFill)
                    Spacer(Modifier.height(16.dp))
                    Text("Leaving your verse", color = goldInk, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                    Spacer(Modifier.height(6.dp))
                    Text("everything saved in your namespace", color = muted, fontSize = 13.sp)
                }
            }
        }
        if (verseExiting) {
            LaunchedEffect(Unit) {
                kotlinx.coroutines.delay(1100)
                HeyVersePlugin.postUi("sleep")
                verseSheet = null
                tab = Tab.Chat
                verseExiting = false
            }
        }
        // "Share a moment" composer — an elegant popup sheet over the feed.
        if (composing) ComposerScreen(onBack = { composing = false }) { composing = false; feedVersion++ }
        // Full-screen peer profile overlay (from Activity / followers).
        profileDid?.let { d ->
            UserProfileScreen(
                did = d,
                onBack = { profileDid = null },
                onMessage = { who -> profileDid = null; openConversation = Chat(who, HeyApi.shortDid(who), "", 0, 0, false) },
            )
        }
        // Full-screen conversation overlay — covers the top bar + dock so the chat
        // gets the whole screen (its own header has the back button).
        LaunchedEffect(openChatDid) {
            openChatDid?.let { openConversation = Chat(it, HeyApi.shortDid(it), "", 0, 0, false); openChatDid = null }
        }
        // Conversation overlay — slides in from the right, slides back out. Keep the
        // last chat during the exit animation so it can play.
        var lastConvo by remember { mutableStateOf<Chat?>(null) }
        LaunchedEffect(openConversation) { openConversation?.let { lastConvo = it } }
        if (openConversation != null) androidx.activity.compose.BackHandler { openConversation = null }
        AnimatedVisibility(
            visible = openConversation != null,
            enter = slideInHorizontally(animationSpec = tween(280)) { it } + fadeIn(tween(160)),
            exit = slideOutHorizontally(animationSpec = tween(260)) { it } + fadeOut(tween(200)),
        ) {
            (openConversation ?: lastConvo)?.let { c ->
                FrostBackground { ConversationScreen(c) { openConversation = null } }
            }
        }
        // Notifications — a frosted-glass card that springs out of the bell (top-right) like a bubble.
        androidx.activity.compose.BackHandler(enabled = showNotifs) { showNotifs = false }
        AnimatedVisibility(
            visible = showNotifs,
            enter = fadeIn(tween(150)) + scaleIn(initialScale = 0.80f, transformOrigin = TransformOrigin(0.93f, 0f), animationSpec = tween(260)),
            exit = fadeOut(tween(170)) + scaleOut(targetScale = 0.85f, transformOrigin = TransformOrigin(0.93f, 0f), animationSpec = tween(190)),
        ) {
            Column(Modifier.fillMaxSize()) {
                // Tap the top strip (where the bell is) to close it again.
                Box(Modifier.statusBarsPadding().fillMaxWidth().height(64.dp).clickable { showNotifs = false })
                Column(
                    Modifier.padding(start = 12.dp, end = 12.dp)
                        .fillMaxWidth().heightIn(max = 520.dp)
                        .clip(RoundedCornerShape(22.dp))
                        .background(bg2.copy(alpha = 0.97f))
                        .background(Brush.verticalGradient(listOf(Color.White.copy(alpha = if (heyLight) 0.16f else 0.06f), Color.Transparent)))
                        .border(1.dp, glassBorder, RoundedCornerShape(22.dp)),
                ) {
                    NotificationsScreen(onOpenProfile = { showNotifs = false; profileDid = it })
                }
                // Tap below the card to dismiss.
                Box(Modifier.weight(1f).fillMaxWidth().clickable { showNotifs = false })
            }
        }
        // 1:1 voice-call overlay — draws above everything (chat/feed/profile) when a call is live.
        // Fades in/out instead of snapping; keeps the last call shown during the exit animation.
        val callState = CallManager.state
        var lastCall by remember { mutableStateOf(callState) }
        LaunchedEffect(callState) { if (callState !is CallManager.State.Idle) lastCall = callState }
        AnimatedVisibility(
            visible = callState !is CallManager.State.Idle,
            enter = fadeIn(tween(260)),
            exit = fadeOut(tween(240)),
        ) { CallOverlay(lastCall) }

        // Show the call UI over the lock screen while a call is live (incoming or active),
        // and CLEAR it the moment the call ends so Hey never shows over the lock outside a
        // call. Pairs with MainActivity.handleCallIntent (the launch path).
        val callActivity = LocalContext.current as? android.app.Activity
        LaunchedEffect(callState) {
            if (Build.VERSION.SDK_INT >= 27 && callActivity != null) {
                val live = callState is CallManager.State.Incoming || callState is CallManager.State.Active
                runCatching { callActivity.setShowWhenLocked(live) }
                if (callState is CallManager.State.Incoming) runCatching { callActivity.setTurnScreenOn(true) }
            }
            // Keep the screen awake for the WHOLE call (voice/video/group) so it never
            // auto-locks mid-call; cleared the instant the call ends.
            val anyCall = callState !is CallManager.State.Idle
            runCatching {
                val w = callActivity?.window ?: return@runCatching
                if (anyCall) w.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                else w.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            }
        }

        // One-time transparency notice when connected via a relay (not a direct link).
        if (showRelayNotice) {
            fun dismissRelay() { showRelayNotice = false; prefs.edit().putBoolean("relay_notice_seen", true).apply() }
            androidx.compose.ui.window.Dialog(onDismissRequest = { dismissRelay() }) {
                Column(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(sheetBg)
                        .border(1.dp, glassBorder, RoundedCornerShape(22.dp)).padding(20.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Hub, null, tint = goldInk, modifier = Modifier.size(24.dp))
                        Spacer(Modifier.width(10.dp))
                        Text("Connected through a relay", color = ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
                    }
                    Spacer(Modifier.height(8.dp))
                    Text(
                        "Your network can't open a direct phone-to-phone link right now, so Hey is routing through a relay. Exactly what that means:",
                        color = muted, fontSize = 13.sp,
                    )
                    Spacer(Modifier.height(14.dp))
                    RelayFact(Icons.Filled.Lock, "End-to-end encrypted", "The relay only ever sees scrambled bytes — never your messages, photos, or calls.")
                    RelayFact(Icons.Filled.Shield, "Post-quantum sealed", "Everything is locked with ML-KEM-768 + X25519 — safe even against future quantum computers.")
                    RelayFact(Icons.Filled.DeleteSweep, "Stores nothing", "It forwards and forgets — no logs of your data, no account, nothing kept.")
                    RelayFact(Icons.Filled.SwapHoriz, "Just a matchmaker", "It introduces your devices; the moment a direct link is possible, your data flows phone-to-phone and the relay drops out.")
                    Spacer(Modifier.height(16.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        OutlinedButton(onClick = { dismissRelay(); tab = Tab.Profile }, modifier = Modifier.weight(1f)) {
                            Text("My own relay", color = ink, fontSize = 13.sp)
                        }
                        Button(
                            onClick = { dismissRelay() },
                            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                            modifier = Modifier.weight(1f),
                        ) { Text("Got it", fontWeight = FontWeight.SemiBold) }
                    }
                }
            }
        }
        // RE-LOCK overlay (post-boot). When the app re-locks during a live session
        // we must NOT unmount the content — the GodotFragment can be hosted only
        // ONCE per process, so disposing it (as the boot early-return did) wedges
        // the engine black and crashes. Instead we keep the content mounted and
        // draw the SAME lock UI as a FULLY OPAQUE overlay on top: it paints the
        // app gradient over everything (privacy — no verse/wallet/chat shows
        // through) and swallows all touches, and it is driven by the very same
        // unlock callback (lockUi). Unlocking flips `unlocked` true → the overlay
        // disappears and the verse is intact; surface pause/resume + the
        // black-screen failsafe then handle plain GL-surface loss as before.
        if (!unlocked && everBooted) {
            Box(
                Modifier.fillMaxSize()
                    .background(Brush.verticalGradient(listOf(bg1, bg2, bg3)))
                    .clickable(
                        indication = null,
                        interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() },
                    ) { /* consume taps so locked content can't be touched */ }
            ) {
                lockUi()
            }
        }
    }
}

/** One transparency fact row in the relay notice: icon + title + plain-language body. */
@Composable
private fun RelayFact(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, body: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.Top) {
        Icon(icon, null, tint = good, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            Text(body, color = muted, fontSize = 11.sp)
        }
    }
}

// ── 1:1 voice call UI ─────────────────────────────────────────────────────────
@Composable
private fun CallOverlay(s: CallManager.State) {
    if (s is CallManager.State.Idle) return
    // Video calls get a dedicated full-screen layout (remote video + self-preview + PiP).
    if (s is CallManager.State.Active && s.video) { VideoCallActive(s); return }
    val name = when (s) {
        is CallManager.State.Outgoing -> s.name
        is CallManager.State.Incoming -> s.name
        is CallManager.State.Active -> s.name
        is CallManager.State.GroupActive -> s.title
        else -> ""
    }
    Box(
        Modifier.fillMaxSize()
            .background(Brush.verticalGradient(listOf(Color(0xFF0A1426), Color(0xFF13233F))))
            .systemBarsPadding(),
        contentAlignment = Alignment.Center,
    ) {
        Column(Modifier.fillMaxSize().padding(28.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Spacer(Modifier.weight(1f))
            Box(Modifier.size(110.dp).clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                if (s is CallManager.State.GroupActive)
                    Icon(Icons.Filled.Groups, null, tint = Navy, modifier = Modifier.size(54.dp))
                else
                    Text(name.take(1).uppercase().ifBlank { "?" }, color = Navy, fontSize = 46.sp, fontWeight = FontWeight.Bold)
            }
            Spacer(Modifier.height(20.dp))
            Text(name.ifBlank { "Unknown" }, color = Color.White, fontSize = 26.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(8.dp))
            when (s) {
                is CallManager.State.Outgoing -> Text("Calling…", color = Color(0xFF9FB2D0), fontSize = 15.sp)
                is CallManager.State.Incoming -> Text(if (s.video) "Incoming video call" else "Incoming voice call", color = Color(0xFF9FB2D0), fontSize = 15.sp)
                is CallManager.State.Active -> CallTimer(s.sinceElapsed)
                is CallManager.State.GroupActive -> {
                    val others = s.participants.count { !it.mine }
                    Text(if (others == 0) "Waiting for others…" else "$others connected", color = Color(0xFF9FB2D0), fontSize = 15.sp)
                    Spacer(Modifier.height(6.dp))
                    CallTimer(s.sinceElapsed)
                    if (s.participants.isNotEmpty()) {
                        Spacer(Modifier.height(18.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            s.participants.take(6).forEach { p ->
                                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                    Box(Modifier.size(46.dp).clip(CircleShape).background(Color(0x33FFFFFF)), Alignment.Center) {
                                        Text((p.name.ifBlank { HeyApi.shortDid(p.did) }).take(1).uppercase(), color = Color.White, fontWeight = FontWeight.Bold)
                                    }
                                    Spacer(Modifier.height(4.dp))
                                    Text(if (p.mine) "You" else p.name.ifBlank { HeyApi.shortDid(p.did) }, color = Color(0xFF9FB2D0), fontSize = 10.sp, maxLines = 1)
                                }
                            }
                        }
                    }
                }
                else -> {}
            }
            Spacer(Modifier.weight(1f))
            when (s) {
                is CallManager.State.Incoming -> {
                    val toneCtx = LocalContext.current
                    // Calm ringtone for the lifetime of the incoming-call screen (stops on accept/decline/timeout).
                    DisposableEffect(s.callId) {
                        CallTone.startIncoming(toneCtx)
                        onDispose { CallTone.stop() }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(56.dp)) {
                        CallButton(Icons.Filled.CallEnd, Color(0xFFE5484D), "Decline") { CallManager.decline() }
                        CallButton(Icons.Filled.Call, Color(0xFF1FAD66), "Accept") { CallManager.accept() }
                    }
                }
                is CallManager.State.Outgoing ->
                    CallButton(Icons.Filled.CallEnd, Color(0xFFE5484D), "Cancel") { CallManager.hangup() }
                is CallManager.State.Active -> {
                    val cctx = LocalContext.current
                    var micGranted by remember {
                        mutableStateOf(
                            androidx.core.content.ContextCompat.checkSelfPermission(cctx, android.Manifest.permission.RECORD_AUDIO)
                                == android.content.pm.PackageManager.PERMISSION_GRANTED
                        )
                    }
                    val micLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
                        micGranted = granted
                        if (granted) VoiceAudio.setMic(cctx, true)
                    }
                    var muted by remember { mutableStateOf(false) }
                    var speaker by remember { mutableStateOf(false) }
                    // Own the audio session for the lifetime of the Active call: start on enter,
                    // stop on dispose (hang-up / remote end → state leaves Active → onDispose).
                    DisposableEffect(s.callId) {
                        VoiceAudio.start(cctx, s.peer, s.isCaller, micGranted)
                        if (!micGranted) micLauncher.launch(android.Manifest.permission.RECORD_AUDIO)
                        onDispose { VoiceAudio.stop(cctx) }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(28.dp), verticalAlignment = Alignment.CenterVertically) {
                        CallButton(if (muted) Icons.Filled.MicOff else Icons.Filled.Mic, Color(0x33FFFFFF), if (muted) "Unmute" else "Mute") {
                            muted = !muted; VoiceAudio.setMic(cctx, !muted && micGranted)
                        }
                        CallButton(Icons.Filled.CallEnd, Color(0xFFE5484D), "Hang up") { CallManager.hangup() }
                        CallButton(if (speaker) Icons.Filled.VolumeUp else Icons.Filled.VolumeDown, Color(0x33FFFFFF), "Speaker") {
                            speaker = !speaker; VoiceAudio.setSpeaker(cctx, speaker)
                        }
                    }
                    // live audio-link probe: tells mic problems apart from transport
                    var audioPeers by remember { mutableStateOf(0) }
                    var linkAge by remember { mutableStateOf(0) }
                    LaunchedEffect(s.callId) {
                        while (true) {
                            audioPeers = HeyApi.voicePeers()
                            linkAge++
                            kotlinx.coroutines.delay(1000)
                        }
                    }
                    if (audioPeers == 0) {
                        Spacer(Modifier.height(10.dp))
                        Text(
                            if (linkAge < 10) "connecting audio…"
                            else "audio link not forming — make sure both phones run the latest Hey",
                            color = Color(0x88FFFFFF), fontSize = 11.sp,
                        )
                    }
                    if (!micGranted) {
                        Spacer(Modifier.height(12.dp))
                        Text(
                            "Allow microphone so you can be heard — tap to open settings",
                            color = Color(0xAAFFD27A), fontSize = 11.sp,
                            modifier = Modifier.clickable {
                                runCatching {
                                    cctx.startActivity(
                                        android.content.Intent(
                                            android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                            android.net.Uri.parse("package:" + cctx.packageName),
                                        ).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                                    )
                                }
                            },
                        )
                    }
                }
                is CallManager.State.GroupActive -> {
                    val cctx = LocalContext.current
                    var micGranted by remember {
                        mutableStateOf(
                            androidx.core.content.ContextCompat.checkSelfPermission(cctx, android.Manifest.permission.RECORD_AUDIO)
                                == android.content.pm.PackageManager.PERMISSION_GRANTED
                        )
                    }
                    val micLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
                        micGranted = granted
                        if (granted) VoiceAudio.setMic(cctx, true)
                    }
                    var muted by remember { mutableStateOf(false) }
                    var speaker by remember { mutableStateOf(false) }
                    DisposableEffect(s.callId) {
                        VoiceAudio.startGroup(cctx, micGranted)
                        if (!micGranted) micLauncher.launch(android.Manifest.permission.RECORD_AUDIO)
                        onDispose { VoiceAudio.stop(cctx) }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(28.dp), verticalAlignment = Alignment.CenterVertically) {
                        CallButton(if (muted) Icons.Filled.MicOff else Icons.Filled.Mic, Color(0x33FFFFFF), if (muted) "Unmute" else "Mute") {
                            muted = !muted; VoiceAudio.setMic(cctx, !muted && micGranted)
                        }
                        CallButton(Icons.Filled.CallEnd, Color(0xFFE5484D), "Leave") { CallManager.hangupGroup() }
                        CallButton(if (speaker) Icons.Filled.VolumeUp else Icons.Filled.VolumeDown, Color(0x33FFFFFF), "Speaker") {
                            speaker = !speaker; VoiceAudio.setSpeaker(cctx, speaker)
                        }
                    }
                    if (!micGranted) {
                        Spacer(Modifier.height(12.dp))
                        Text("Allow microphone so you can be heard", color = Color(0x88FFFFFF), fontSize = 11.sp)
                    }
                }
                else -> {}
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** Full-screen video-call layout: remote video fills the screen, the local camera is
 *  a small draggable self-view, controls overlay the bottom, and "Float" drops the call
 *  into a Picture-in-Picture window so the user can use the phone during the call. Audio
 *  (VoiceAudio) runs alongside; if the path degrades to relay mid-call it demotes to voice. */
@Composable
private fun VideoCallActive(s: CallManager.State.Active) {
    val ctx = LocalContext.current
    val lifecycleOwner = ctx as? androidx.lifecycle.LifecycleOwner
    var camGranted by remember {
        mutableStateOf(androidx.core.content.ContextCompat.checkSelfPermission(ctx, android.Manifest.permission.CAMERA) == android.content.pm.PackageManager.PERMISSION_GRANTED)
    }
    var micGranted by remember {
        mutableStateOf(androidx.core.content.ContextCompat.checkSelfPermission(ctx, android.Manifest.permission.RECORD_AUDIO) == android.content.pm.PackageManager.PERMISSION_GRANTED)
    }
    val permLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { res ->
        res[android.Manifest.permission.CAMERA]?.let { camGranted = it }
        res[android.Manifest.permission.RECORD_AUDIO]?.let { micGranted = it }
    }
    var muted by remember { mutableStateOf(false) }
    var camOff by remember { mutableStateOf(false) }
    var videoPeers by remember { mutableStateOf(0) }
    var linkAge by remember { mutableStateOf(0) }
    var inPip by remember { mutableStateOf(false) }
    val previewView = remember {
        androidx.camera.view.PreviewView(ctx).apply { scaleType = androidx.camera.view.PreviewView.ScaleType.FILL_CENTER }
    }
    var remoteSurface by remember { mutableStateOf<android.view.Surface?>(null) }
    var remoteAspect by remember { mutableStateOf(0f) }

    LaunchedEffect(Unit) {
        val need = buildList {
            if (!camGranted) add(android.Manifest.permission.CAMERA)
            if (!micGranted) add(android.Manifest.permission.RECORD_AUDIO)
        }
        if (need.isNotEmpty()) permLauncher.launch(need.toTypedArray())
    }
    // Track PiP so we hide the chrome when floating (just the remote video shows).
    LaunchedEffect(s.callId) {
        while (true) {
            inPip = (ctx as? android.app.Activity)?.isInPictureInPictureMode == true
            videoPeers = HeyApi.videoPeers()
            remoteAspect = VideoCall.remoteAspect()
            linkAge++
            kotlinx.coroutines.delay(1000)
        }
    }
    // Demote to voice if the path drops to relay mid-call (video is direct-only).
    LaunchedEffect(s.callId) {
        while (true) {
            kotlinx.coroutines.delay(3000)
            if (HeyApi.contactTransport(s.peer) == "relay") { CallManager.demoteToVoice(); break }
        }
    }

    Box(
        Modifier.fillMaxSize().background(Color.Black)
            // Absorb ALL touches so a tap on the video can't fall through to the chat/photo
            // BEHIND the call overlay (it was touch-transparent). Child controls, declared
            // later in this Box, still receive their own taps.
            .pointerInput(Unit) {
                awaitPointerEventScope { while (true) { awaitPointerEvent().changes.forEach { it.consume() } } }
            },
        contentAlignment = Alignment.Center,
    ) {
        // Remote video — aspect-correct (fit/letterbox), NOT stretched or cropped.
        // Constrain to the decoded video's real ratio once known; fill while waiting
        // for the first keyframe (black until then).
        androidx.compose.ui.viewinterop.AndroidView(
            factory = { c ->
                // TextureView (not SurfaceView): a SurfaceView's video layer does NOT
                // follow the window into the PiP shrink, so it freezes on the last frame
                // until restored. A TextureView composites inside the view hierarchy and
                // keeps rendering through PiP.
                android.view.TextureView(c).apply {
                    surfaceTextureListener = object : android.view.TextureView.SurfaceTextureListener {
                        override fun onSurfaceTextureAvailable(st: android.graphics.SurfaceTexture, w: Int, h: Int) {
                            android.util.Log.i("video", "remote texture available ${w}x${h}")
                            remoteSurface = android.view.Surface(st)
                        }
                        override fun onSurfaceTextureSizeChanged(st: android.graphics.SurfaceTexture, w: Int, h: Int) {}
                        override fun onSurfaceTextureDestroyed(st: android.graphics.SurfaceTexture): Boolean {
                            android.util.Log.i("video", "remote texture destroyed")
                            remoteSurface = null
                            return true
                        }
                        override fun onSurfaceTextureUpdated(st: android.graphics.SurfaceTexture) {}
                    }
                }
            },
            // Aspect-correct (letterbox) in BOTH fullscreen and PiP. With the PiP window
            // aspect set to the real video ratio (Float button below), this fits the window
            // exactly — no stretch.
            modifier = if (remoteAspect > 0f)
                Modifier.aspectRatio(remoteAspect)
            else
                Modifier.fillMaxSize(),
        )

        // Local self-view (hidden in PiP).
        if (!inPip) {
            androidx.compose.ui.viewinterop.AndroidView(
                factory = { previewView },
                modifier = Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(14.dp)
                    .size(104.dp, 156.dp).clip(RoundedCornerShape(12.dp)),
            )
        }

        // Start audio + the video TRANSPORT immediately (NOT gated on the surface) so
        // the link forms exactly like voice; the decoder attaches when the surface is
        // ready (next line). Stop both on dispose.
        DisposableEffect(s.callId, camGranted, micGranted) {
            VoiceAudio.start(ctx, s.peer, s.isCaller, micGranted)
            if (lifecycleOwner != null) {
                VideoCall.start(ctx, lifecycleOwner, s.peer, camGranted, previewView)
            }
            onDispose {
                VideoCall.stop()
                VoiceAudio.stop(ctx)
            }
        }
        // Attach the remote render surface when it becomes available (decoupled from the link).
        LaunchedEffect(remoteSurface) {
            remoteSurface?.let { VideoCall.setRemoteSurface(it) }
        }

        if (!inPip) {
            Column(Modifier.align(Alignment.TopStart).statusBarsPadding().padding(16.dp)) {
                Text(s.name.ifBlank { "Video call" }, color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
                CallTimer(s.sinceElapsed)
                if (videoPeers == 0) {
                    Text(
                        if (linkAge < 12) "connecting video…" else "video link not forming",
                        color = Color(0xAAFFFFFF), fontSize = 12.sp,
                    )
                }
            }
            Row(
                Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(22.dp),
                horizontalArrangement = Arrangement.spacedBy(18.dp), verticalAlignment = Alignment.CenterVertically,
            ) {
                CallButton(if (muted) Icons.Filled.MicOff else Icons.Filled.Mic, Color(0x33FFFFFF), if (muted) "Unmute" else "Mute") {
                    muted = !muted; VoiceAudio.setMic(ctx, !muted && micGranted)
                }
                CallButton(if (camOff) Icons.Filled.VideocamOff else Icons.Filled.Videocam, Color(0x33FFFFFF), if (camOff) "Cam on" else "Cam off") {
                    camOff = !camOff; VideoCall.setVideoMuted(camOff)
                }
                CallButton(Icons.Filled.Cameraswitch, Color(0x33FFFFFF), "Flip") { VideoCall.flipCamera() }
                CallButton(Icons.Filled.CallEnd, Color(0xFFE5484D), "Hang up") { CallManager.hangup() }
                CallButton(Icons.Filled.PictureInPictureAlt, Color(0x33FFFFFF), "Float") {
                    runCatching {
                        val act = ctx as? android.app.Activity ?: return@runCatching
                        // Match the PiP window to the REAL remote video ratio so it doesn't
                        // stretch; clamp to Android's allowed PiP range to avoid a crash.
                        val vw = VideoCall.remoteW; val vh = VideoCall.remoteH
                        val r = if (vw > 0 && vh > 0) vw.toFloat() / vh.toFloat() else 9f / 16f
                        val cr = r.coerceIn(0.42f, 2.38f)
                        val params = android.app.PictureInPictureParams.Builder()
                            .setAspectRatio(android.util.Rational((cr * 1000).toInt(), 1000)).build()
                        act.enterPictureInPictureMode(params)
                    }
                }
            }
            if (!camGranted) {
                Text(
                    "Allow camera to be seen", color = Color(0xAAFFD27A), fontSize = 11.sp,
                    modifier = Modifier.align(Alignment.BottomStart).navigationBarsPadding().padding(16.dp, 80.dp),
                )
            }
        }
    }
}

@Composable
private fun CallTimer(sinceElapsed: Long) {
    var now by remember { mutableStateOf(android.os.SystemClock.elapsedRealtime()) }
    LaunchedEffect(sinceElapsed) {
        while (true) { now = android.os.SystemClock.elapsedRealtime(); kotlinx.coroutines.delay(1000) }
    }
    val secs = ((now - sinceElapsed) / 1000).coerceAtLeast(0)
    Text("%d:%02d".format(secs / 60, secs % 60), color = Color(0xFF9FB2D0), fontSize = 16.sp)
}

@Composable
private fun CallButton(icon: androidx.compose.ui.graphics.vector.ImageVector, bg: Color, label: String, onClick: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(68.dp).clip(CircleShape).background(bg).clickable { onClick() }, contentAlignment = Alignment.Center) {
            Icon(icon, label, tint = Color.White, modifier = Modifier.size(30.dp))
        }
        Spacer(Modifier.height(8.dp))
        Text(label, color = Color(0xFFB9C6DD), fontSize = 12.sp)
    }
}

@Composable
private fun FloatingDock(tab: Tab, unread: Int, online: Boolean, onSelect: (Tab) -> Unit, onVerse: (String) -> Unit = {}, modifier: Modifier = Modifier) {
    Row(
        modifier
            .clip(RoundedCornerShape(28.dp))
            // Heavier frost: nearly-opaque base + a milky sheen on top so it reads
            // as a solid frosted panel instead of a see-through pane.
            .background(bg2.copy(alpha = 0.95f))
            .background(
                Brush.verticalGradient(
                    listOf(
                        Color.White.copy(alpha = if (heyLight) 0.22f else 0.09f),
                        Color.White.copy(alpha = if (heyLight) 0.06f else 0.02f),
                    )
                )
            )
            .border(1.dp, glassBorder, RoundedCornerShape(28.dp))
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // smooth morph: regular dock pops down, the game dock pops up
        AnimatedContent(
            targetState = tab == Tab.Verse && BuildConfig.VERSE_ENABLED,
            transitionSpec = {
                (slideInVertically(tween(260)) { it / 2 } + fadeIn(tween(260))) togetherWith
                    (slideOutVertically(tween(170)) { it / 2 } + fadeOut(tween(140)))
            },
            label = "dockmorph",
        ) { verseMode ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (verseMode) {
                    // In the verse, the dock IS the game's controls.
                    DockItem(false, Icons.Filled.Face, "Avatar", 0) { onVerse("avatar") }
                    DockItem(false, Icons.Filled.Public, "Worlds", 0) { onVerse("worlds") }
                    DockItem(false, Icons.Filled.PersonAdd, "Invite", 0) { onVerse("invite") }
                    DockItem(false, Icons.Filled.Inventory2, "Library", 0) { onVerse("library") }
                    DockItem(false, Icons.Filled.PowerSettingsNew, "Exit", 0) { onVerse("exit") }
                } else {
                    DockItem(tab == Tab.Chat, Icons.Filled.Forum, "Chat", unread) { onSelect(Tab.Chat) }
                    DockItem(tab == Tab.Feed, Icons.Filled.DynamicFeed, "Feed", 0) { onSelect(Tab.Feed) }
                    // Activity moved to a top-right bell — Verse took its slot.
                    DockItem(tab == Tab.Verse, Icons.Filled.Public, "Verse", 0) { onSelect(Tab.Verse) }
                    DockItem(tab == Tab.Wallet, Icons.Filled.AccountBalanceWallet, "Wallet", 0) { onSelect(Tab.Wallet) }
                    DockItem(tab == Tab.Profile, Icons.Filled.AccountCircle, "You", 0, status = online) { onSelect(Tab.Profile) }
                }
            }
        }
    }
}

// ── Hey Verse dock sheets — the game's settings in Hey's popup module ───────

@Composable
private fun VerseSheetTitle(title: String, sub: String) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 22.dp)) {
        Text(title, color = goldInk, fontWeight = FontWeight.Bold, fontSize = 19.sp)
        Spacer(Modifier.height(3.dp))
        Text(sub, color = muted, fontSize = 13.sp)
        Spacer(Modifier.height(14.dp))
    }
}

@Composable
private fun VerseCmdButton(label: String, cmd: String) {
    Button(
        onClick = { HeyVersePlugin.postUi(cmd) },
        colors = ButtonDefaults.buttonColors(containerColor = glassFill, contentColor = ink),
        shape = RoundedCornerShape(18.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, glassBorder),
    ) { Text(label, fontSize = 14.sp) }
}

/** Sash's FAQ — about Elacity (elacitylabs.com: "The World Computer
 *  Marketplace"), opened by tapping him in Ela City. */
@Composable
private fun VerseSashFaqSheet(onClose: () -> Unit = {}) {
    Box(Modifier.fillMaxWidth()) {
        VerseSheetTitle("Sash · about Elacity", "the creator of Elacity answers a few questions")
        IconButton(onClick = onClose, modifier = Modifier.align(Alignment.TopEnd).padding(end = 10.dp)) {
            Icon(Icons.Filled.Close, "Close", tint = muted)
        }
    }
    val faq = listOf(
        "What is Elacity?" to
            "The World Computer Marketplace — a place where digital things are truly owned, traded and enjoyed by people, not platforms. It runs on Elastos.",
        "What does \"truly owned\" mean?" to
            "Your assets live in your own namespace on your own devices, secured by the chain. No platform can take them away or lock you in.",
        "What is dDRM?" to
            "Decentralized DRM: media travels encrypted, and owning the access token releases the key. Files stay yours and play anywhere — no central server.",
        "What can I buy on ela.city?" to
            "Digital assets — art, video, 3D models. Soon: wearables for your robot, furniture and assets for your home and worlds here in the Verse.",
        "What is PC2?" to
            "Your Personal Cloud Computer — your own corner of the world computer that runs your spaces and serves your content.",
        "Why a city?" to
            "Because a marketplace should feel like a place. Walk around, meet people, window-shop — the mall opens its stores soon!",
    )
    Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()).padding(horizontal = 22.dp)) {
        for ((q, a) in faq) {
            Text(q, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            Spacer(Modifier.height(3.dp))
            Text(a, color = muted, fontSize = 13.sp, lineHeight = 18.sp)
            Spacer(Modifier.height(12.dp))
        }
        Text("more at elacitylabs.com", color = goldInk, fontSize = 12.sp)
    }
}

/** Almost-fullscreen avatar editor: the TOP is transparent, so the game's
 *  live close-up of the robot shows through while you edit. */
@Composable
private fun VerseAvatarEditor(onClose: () -> Unit) {
    LaunchedEffect(Unit) { HeyVersePlugin.postUi("edit_on") }
    Column(Modifier.fillMaxSize()) {
        // see-through window onto the game (tap to finish)
        Box(Modifier.fillMaxWidth().weight(1f).clickable { onClose() })
        // COMPACT bottom panel: a short, SCROLLABLE control strip so the robot
        // close-up above stays big. Header + Done are fixed; the rows scroll.
        Column(
            Modifier.fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 26.dp, topEnd = 26.dp))
                .background(bg2.copy(alpha = 0.97f))
                .navigationBarsPadding()
                .padding(horizontal = 22.dp, vertical = 12.dp)
        ) {
            Text("Edit avatar", color = goldInk, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Spacer(Modifier.height(8.dp))
            Column(Modifier.heightIn(max = 196.dp).verticalScroll(rememberScrollState())) {
                Text("Finish", color = goldInk, fontSize = 12.sp)
                Spacer(Modifier.height(5.dp))
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    VerseCmdButton("Gold", "finish:gold")
                    VerseCmdButton("Silver", "finish:silver")
                    VerseCmdButton("Obsidian", "finish:obsidian")
                    VerseCmdButton("Classic", "finish:classic")
                }
                Spacer(Modifier.height(9.dp))
                Text("Headwear", color = goldInk, fontSize = 12.sp)
                Spacer(Modifier.height(5.dp))
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    VerseCmdButton("None", "wear:")
                    for (h in VerseCatalogData.hats) VerseCmdButton(h.name, "wear:${h.id}")
                }
                Spacer(Modifier.height(9.dp))
                Text("Eyes", color = goldInk, fontSize = 12.sp)
                Spacer(Modifier.height(5.dp))
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    VerseCmdButton("Round", "eyes_set:0")
                    VerseCmdButton("Oval", "eyes_set:1")
                    VerseCmdButton("Happy", "eyes_set:2")
                }
                Spacer(Modifier.height(9.dp))
                Text("Antenna", color = goldInk, fontSize = 12.sp)
                Spacer(Modifier.height(5.dp))
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    VerseCmdButton("Zigzag", "fins_set:0")
                    VerseCmdButton("Straight", "fins_set:1")
                    VerseCmdButton("Tall", "fins_set:2")
                    VerseCmdButton("None", "fins_set:3")
                }
                Spacer(Modifier.height(8.dp))
                Text("Eyes, antenna & finishes are owned parts — more to collect as NFTs.", color = muted, fontSize = 11.sp)
            }
            Spacer(Modifier.height(10.dp))
            Button(
                onClick = onClose,
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                shape = RoundedCornerShape(18.dp),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Done", fontWeight = FontWeight.SemiBold) }
        }
    }
}

/** Shipped (non-Verse) builds show this when the Verse tab is tapped. */
@Composable
private fun VerseComingSoon(topPad: androidx.compose.ui.unit.Dp) {
    Box(
        Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(bg1, bg2, bg3))),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.padding(36.dp)) {
            Icon(Icons.Filled.Public, null, tint = Gold, modifier = Modifier.size(64.dp))
            Spacer(Modifier.height(18.dp))
            Text("Hey Verse", color = goldInk, fontWeight = FontWeight.Bold, fontSize = 26.sp)
            Spacer(Modifier.height(8.dp))
            Text("Coming soon", color = Gold, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(14.dp))
            Text(
                "Your own 3D world — walk it, build it with premium pieces, dress your avatar, and truly own your space. Launching shortly.",
                color = muted, fontSize = 14.sp,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center, lineHeight = 20.sp,
            )
        }
    }
}

@Composable
private fun VerseWorldsSheet() {
    VerseSheetTitle("Worlds", "Spaces you can visit — and one that is truly yours.")
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 22.dp).clip(RoundedCornerShape(16.dp))
            .background(glassFill).border(1.dp, glassBorder, RoundedCornerShape(16.dp)).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(Icons.Filled.Home, null, tint = goldInk)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text("My Home", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
            Text("your world", color = good, fontSize = 12.sp)
        }
        Button(
            onClick = { HeyVersePlugin.postUi("goto_home") },
            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
            shape = RoundedCornerShape(16.dp),
        ) { Text("Visit", fontSize = 13.sp) }
    }
    Spacer(Modifier.height(8.dp))
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 22.dp).clip(RoundedCornerShape(16.dp))
            .background(glassFill).border(1.dp, glassBorder, RoundedCornerShape(16.dp)).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(Icons.Filled.LocationCity, null, tint = goldInk)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text("Ela City", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
            Text("futuristic robot city · vendors + mall", color = muted, fontSize = 12.sp)
        }
        Button(
            onClick = { HeyVersePlugin.postUi("goto_city") },
            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
            shape = RoundedCornerShape(16.dp),
        ) { Text("Visit", fontSize = 13.sp) }
    }
    Spacer(Modifier.height(12.dp))
    Text("Lighting", color = muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 22.dp))
    Spacer(Modifier.height(6.dp))
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 22.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        VerseCmdButton("Day", "preset_day")
        VerseCmdButton("Sunset", "preset_sunset")
        VerseCmdButton("Night", "preset_night")
    }
    Spacer(Modifier.height(16.dp))
    Text(
        "Community worlds — visit and buy spaces others created (shops, galleries, malls…) — coming soon.",
        color = muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 22.dp)
    )
}

@Composable
private fun VerseInviteSheet() {
    VerseSheetTitle("Invite a friend", "Live visits — they walk in as their own ELAnaut. Re-invite after a disconnect.")
    val contacts by produceState(initialValue = emptyList<Chat>()) {
        value = withContext(Dispatchers.IO) {
            runCatching { HeyApi.chats().filter { !it.isGroup } }.getOrDefault(emptyList())
        }
    }
    if (contacts.isEmpty()) {
        Text("No contacts yet — add friends in Chat first.", color = muted, fontSize = 13.sp,
            modifier = Modifier.padding(horizontal = 22.dp))
    }
    Column(Modifier.fillMaxWidth().padding(horizontal = 22.dp)) {
        for (c in contacts.take(12)) {
            Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(c.name, color = ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                Button(
                    onClick = { HeyVersePlugin.inviteContact(c.id) },
                    colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                    shape = RoundedCornerShape(16.dp),
                ) { Text("Invite", fontSize = 13.sp) }
            }
        }
    }
}

private data class VerseNft(val name: String, val image: String, val contract: String, val id: String)

/** Every NFT the wallet owns on ESC — all ERC-721/1155 contracts (everything
 *  traded on ela.city), enumerated via the chain explorer's REST API. */
private fun fetchEscNfts(addr: String): List<VerseNft> {
    val out = ArrayList<VerseNft>()
    return runCatching {
        val url = java.net.URL("https://esc.elastos.io/api/v2/addresses/$addr/nft?type=ERC-721%2CERC-1155")
        val conn = url.openConnection() as java.net.HttpURLConnection
        conn.connectTimeout = 8000
        conn.readTimeout = 8000
        conn.setRequestProperty("Accept", "application/json")
        val body = conn.inputStream.bufferedReader().readText()
        val items = JSONObject(body).optJSONArray("items") ?: return@runCatching out
        for (i in 0 until items.length()) {
            val o = items.getJSONObject(i)
            val meta = o.optJSONObject("metadata")
            val name = (meta?.optString("name").orEmpty())
                .ifBlank { o.optJSONObject("token")?.optString("name").orEmpty() }
                .ifBlank { "NFT #${o.optString("id")}" }
            val img = o.optString("image_url").ifBlank { meta?.optString("image").orEmpty() }
            out.add(VerseNft(name, img, o.optJSONObject("token")?.optString("address").orEmpty(), o.optString("id")))
        }
        out
    }.getOrDefault(out)
}

/** External NFT art lives on IPFS, NOT in our in-process content store — route
 *  ipfs:// through a SELF-HOSTABLE gateway (HeyApi.resolveIpfs reads the
 *  user-overridable ipfs-gateway row); https images load directly. */
private fun nftImageModel(raw: String): Any =
    if (raw.startsWith("ipfs://") || raw.startsWith("ipfs/")) HeyApi.resolveIpfs(raw) else raw

private fun fetchNftImageBytes(raw: String): ByteArray {
    val url = if (raw.startsWith("ipfs://") || raw.startsWith("ipfs/")) HeyApi.resolveIpfs(raw) else raw
    if (!url.startsWith("http")) return ByteArray(0)
    return runCatching { java.net.URL(url).openStream().use { it.readBytes() } }.getOrDefault(ByteArray(0))
}

private fun rarityColor(r: String): Color = when (r) {
    "Legendary" -> Color(0xFFFFB74D)
    "Epic" -> Color(0xFFBA68C8)
    "Rare" -> Color(0xFF64B5F6)
    "Uncommon" -> Color(0xFF81C784)
    else -> Color(0xFFB0BEC5)
}

@Composable
private fun VerseLibrarySheet(onPlace: (String) -> Unit) {
    VerseSheetTitle("Library", "Place objects in your world (drag to position · snaps to a grid) — or hang an NFT.")
    var objKind by remember { mutableStateOf("") }
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 22.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        for ((k, label) in (VerseCatalogData.objectKinds + listOf("building" to "🏠 Buildings"))) {
            val sel = k == objKind
            Button(
                onClick = { objKind = k },
                colors = ButtonDefaults.buttonColors(containerColor = if (sel) Gold else glassFill, contentColor = if (sel) Navy else ink),
                shape = RoundedCornerShape(16.dp),
                contentPadding = PaddingValues(horizontal = 14.dp, vertical = 6.dp),
            ) { Text(label, fontSize = 13.sp) }
        }
    }
    Spacer(Modifier.height(10.dp))
    val rows = if (objKind == "building") VerseBuildingData.items
               else VerseCatalogData.objects.filter { objKind == "" || it.kind == objKind }
    LazyColumn(Modifier.heightIn(max = 300.dp).padding(horizontal = 22.dp)) {
        items(rows) { obj ->
            Row(Modifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(obj.name, color = ink, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(obj.rarity + " · own ×1", color = rarityColor(obj.rarity), fontSize = 11.sp)
                }
                Button(
                    onClick = { onPlace(obj.id) },
                    colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                    shape = RoundedCornerShape(16.dp),
                ) { Text("Place", fontSize = 13.sp) }
            }
        }
    }
    Spacer(Modifier.height(14.dp))
    Text("Your NFTs (ESC) — hang as a painting", color = goldInk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 22.dp))
    Spacer(Modifier.height(6.dp))
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val addr = remember { HeyApi.walletAddress(ctx) }
    val nfts by produceState<List<VerseNft>?>(initialValue = null) {
        value = withContext(Dispatchers.IO) {
            if (addr == null) emptyList() else fetchEscNfts(addr)
        }
    }
    when {
        nfts == null -> Row(
            Modifier.fillMaxWidth().padding(horizontal = 22.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            CircularProgressIndicator(color = Gold, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
            Spacer(Modifier.width(10.dp))
            Text("reading your wallet on ESC…", color = muted, fontSize = 13.sp)
        }
        nfts!!.isEmpty() -> Text(
            "No NFTs found for this wallet on ESC yet — anything you collect on ela.city will appear here.",
            color = muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 22.dp)
        )
        else -> LazyColumn(Modifier.heightIn(max = 380.dp).padding(horizontal = 22.dp)) {
            items(nfts!!.take(40)) { nft ->
                Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (nft.image.isNotBlank()) {
                        AsyncImage(
                            model = nftImageModel(nft.image), contentDescription = null,
                            contentScale = ContentScale.Crop,
                            modifier = Modifier.size(46.dp).clip(RoundedCornerShape(10.dp)).background(glassFill)
                        )
                    } else {
                        Box(Modifier.size(46.dp).clip(RoundedCornerShape(10.dp)).background(glassFill))
                    }
                    Spacer(Modifier.width(10.dp))
                    Text(nft.name, color = ink, fontSize = 14.sp, maxLines = 1,
                        overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    Button(
                        onClick = {
                            scope.launch(Dispatchers.IO) {
                                val bytes = fetchNftImageBytes(nft.image)
                                if (bytes.isNotEmpty()) {
                                    val f = java.io.File(ctx.cacheDir, "verse_nft_${(nft.contract + nft.id).hashCode()}.img")
                                    f.writeBytes(bytes)
                                    HeyVersePlugin.postUi("hang:${f.absolutePath}")
                                }
                            }
                        },
                        enabled = nft.image.isNotBlank(),
                        colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                        shape = RoundedCornerShape(16.dp),
                    ) { Text("Hang", fontSize = 13.sp) }
                }
            }
        }
    }
    Spacer(Modifier.height(12.dp))
    Text(
        "Coming soon: placeable .ddrm assets from Elacity (pets, furniture, kitchens…) owned in your namespace.",
        color = muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 22.dp)
    )
}

@Composable
private fun DockItem(selected: Boolean, icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, badge: Int, status: Boolean? = null, onClick: () -> Unit) {
    // Smoothly cross-fade the highlight + grow/shrink the pill as selection moves,
    // so the gold highlight appears to flow from one tab to the next.
    val hi by animateColorAsState(if (selected) Gold.copy(alpha = 0.18f) else Color.Transparent, tween(280), label = "dockHi")
    Row(
        Modifier.clip(RoundedCornerShape(20.dp))
            .background(hi)
            .clickable { onClick() }
            .animateContentSize(animationSpec = tween(280))
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            if (badge > 0) {
                BadgedBox(badge = { Badge(containerColor = Like) { Text("$badge") } }) {
                    Icon(icon, label, tint = if (selected) goldInk else muted, modifier = Modifier.size(22.dp))
                }
            } else {
                Icon(icon, label, tint = if (selected) goldInk else muted, modifier = Modifier.size(22.dp))
            }
            // Connection status dot (carrier online = green, else red) over Profile.
            if (status != null) {
                Box(
                    Modifier.align(Alignment.TopEnd).offset(x = 2.dp, y = (-2).dp)
                        .size(9.dp).clip(CircleShape)
                        .background(if (status) Color(0xFF35C759) else Color(0xFFE5484D))
                        .border(1.5.dp, bg2, CircleShape)
                )
            }
        }
        if (selected) {
            Spacer(Modifier.width(6.dp))
            Text(label, color = goldInk, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        }
    }
}

// ── feed ─────────────────────────────────────────────────────────────────────

@Composable
fun FeedScreen(version: Int, feedRev: Long, myDid: String, topPad: Dp = 12.dp, onOpenProfile: (String) -> Unit) {
    var posts by remember { mutableStateOf<List<Post>>(emptyList()) }
    var firstLoad by remember { mutableStateOf(true) }
    var localTick by remember { mutableStateOf(0) }
    LaunchedEffect(version, feedRev, localTick) {
        posts = withContext(Dispatchers.IO) { runCatching { HeyApi.feed(50) }.getOrDefault(emptyList()) }
        firstLoad = false
    }
    when {
        firstLoad -> Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator(color = goldInk) }
        posts.isEmpty() -> Box(Modifier.fillMaxSize(), Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(Icons.Filled.PhotoCamera, null, tint = muted, modifier = Modifier.size(48.dp))
                Spacer(Modifier.height(12.dp))
                Text("Your feed is empty", color = ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                Text("Tap + to share a photo.", color = muted)
            }
        }
        else -> LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = topPad, bottom = 96.dp)) {
            items(posts, key = { it.id }) { PostCard(it, feedRev, myDid, onChanged = { localTick++ }, onOpenProfile = onOpenProfile) }
        }
    }
}

@Composable
private fun Avatar(avatarCid: String, did: String, size: Int, onClick: (() -> Unit)? = null) {
    var mod = Modifier.size(size.dp).clip(RoundedCornerShape((size / 2).dp))
    if (onClick != null) mod = mod.clickable { onClick() }
    if (avatarCid.isNotBlank()) {
        AsyncImage(model = HeyApi.mediaUri(avatarCid), contentDescription = null, contentScale = ContentScale.Crop,
            modifier = mod.background(Color.Black.copy(alpha = 0.2f)))
    } else {
        Box(mod.background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
            Text(did.removePrefix("did:key:z").take(1).uppercase(), color = Navy, fontWeight = FontWeight.Bold, fontSize = (size * 0.45f).sp)
        }
    }
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
fun PostCard(post: Post, feedRev: Long, myDid: String, onChanged: () -> Unit, onOpenProfile: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var reactions by remember { mutableStateOf(Reactions(0, false, 0)) }
    var comments by remember { mutableStateOf<List<Comment>>(emptyList()) }
    var commentText by remember { mutableStateOf("") }
    var replyTo by remember { mutableStateOf<Comment?>(null) }
    var showCommentBox by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf(false) }
    var editText by remember { mutableStateOf(post.caption) }
    var zoomCid by remember { mutableStateOf<String?>(null) }
    var showTip by remember { mutableStateOf(false) }
    val mine = post.author == myDid

    LaunchedEffect(post.id, feedRev) {
        withContext(Dispatchers.IO) { runCatching { reactions = HeyApi.reactions(post.id); comments = HeyApi.comments(post.id) } }
    }
    fun reloadComments() {
        scope.launch { comments = withContext(Dispatchers.IO) { runCatching { HeyApi.comments(post.id) }.getOrDefault(comments) } }
    }

    Column(
        Modifier.fillMaxWidth().padding(vertical = 8.dp).glass()
            .combinedClickable(onClick = {}, onLongClick = { if (mine) menu = true })
            .padding(14.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Avatar(post.authorAvatar, post.author, 36) { if (!mine) onOpenProfile(post.author) }
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f).then(if (!mine) Modifier.clickable { onOpenProfile(post.author) } else Modifier)) {
                Text(post.authorName.ifBlank { HeyApi.shortDid(post.author) }, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                if (post.ts > 0) Text(relativeTime(post.ts), color = muted, fontSize = 11.sp)
            }
            if (mine) {
                Box {
                    IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, "More", tint = muted) }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(text = { Text("Edit caption") }, onClick = { menu = false; editing = true })
                        DropdownMenuItem(text = { Text("Delete post", color = Like) }, onClick = {
                            menu = false
                            scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.deletePost(post.id) } }; onChanged() }
                        })
                    }
                }
            }
        }
        if (post.media.isNotEmpty()) {
            Spacer(Modifier.height(10.dp))
            MediaCarousel(post.media) { cid -> zoomCid = cid }
            Spacer(Modifier.height(10.dp))
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(
                if (reactions.liked) Icons.Filled.Favorite else Icons.Outlined.FavoriteBorder,
                "Like", tint = if (reactions.liked) Like else ink,
                modifier = Modifier.size(26.dp).clickable {
                    scope.launch { reactions = withContext(Dispatchers.IO) { HeyApi.toggleLike(post.id) } }
                }
            )
            if (reactions.likeCount > 0) { Spacer(Modifier.width(6.dp)); Text("${reactions.likeCount}", color = ink, fontSize = 14.sp) }
            Spacer(Modifier.width(18.dp))
            Icon(Icons.Outlined.ChatBubbleOutline, "Comment", tint = if (showCommentBox) goldInk else ink,
                modifier = Modifier.size(24.dp).clickable { showCommentBox = !showCommentBox; if (!showCommentBox) replyTo = null })
            if (comments.isNotEmpty()) { Spacer(Modifier.width(6.dp)); Text("${comments.size}", color = ink, fontSize = 14.sp) }
            // Tip the author — by identity, no address needed (resolved via their profile).
            if (!mine) {
                Spacer(Modifier.weight(1f))
                Icon(Icons.Filled.Paid, "Tip", tint = goldInk, modifier = Modifier.size(24.dp).clickable { showTip = true })
            }
        }
        if (showTip) TipSheet(post.author, post.authorName.ifBlank { HeyApi.shortDid(post.author) }) { showTip = false }
        if (post.caption.isNotBlank()) { Spacer(Modifier.height(8.dp)); Text(post.caption, color = ink, fontSize = 15.sp) }

        // Existing comments always show; the write field opens only on tap (the
        // comment icon, or "Reply"). Replies render indented under their parent.
        if (comments.isNotEmpty()) {
            Spacer(Modifier.height(10.dp))
            val tops = comments.filter { it.parent.isBlank() }
            tops.forEach { c ->
                CommentRow(c, indent = false) { replyTo = c; showCommentBox = true }
                comments.filter { it.parent == c.id }.forEach { r -> CommentRow(r, indent = true) { replyTo = c; showCommentBox = true } }
            }
        }
        if (showCommentBox || replyTo != null) {
            replyTo?.let { r ->
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 4.dp)) {
                    Text("Replying to ${r.authorName.ifBlank { HeyApi.shortDid(r.author) }}", color = goldInk, fontSize = 11.sp, modifier = Modifier.weight(1f))
                    TextButton(onClick = { replyTo = null }) { Text("Cancel", color = muted, fontSize = 11.sp) }
                }
            }
            Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = commentText, onValueChange = { commentText = it },
                    placeholder = { Text(if (replyTo != null) "Reply…" else "Add a comment…", color = muted, fontSize = 13.sp) },
                    modifier = Modifier.weight(1f), singleLine = true,
                    colors = glassFieldColors(), textStyle = androidx.compose.ui.text.TextStyle(color = ink, fontSize = 14.sp)
                )
                TextButton(onClick = {
                    val t = commentText.trim(); if (t.isEmpty()) return@TextButton
                    val parent = replyTo?.id ?: ""
                    commentText = ""; replyTo = null; showCommentBox = false
                    scope.launch {
                        withContext(Dispatchers.IO) { runCatching { HeyApi.addComment(post.id, t, parent) } }
                        reloadComments()
                    }
                }) { Text("Send", color = goldInk) }
            }
        }
    }

    if (editing) {
        AlertDialog(
            onDismissRequest = { editing = false },
            title = { Text("Edit caption", color = ink) },
            text = {
                OutlinedTextField(value = editText, onValueChange = { editText = it }, colors = glassFieldColors(),
                    textStyle = androidx.compose.ui.text.TextStyle(color = ink))
            },
            confirmButton = {
                TextButton(onClick = {
                    editing = false
                    scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.editPost(post.id, editText) } }; onChanged() }
                }) { Text("Save", color = goldInk) }
            },
            dismissButton = { TextButton(onClick = { editing = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
    zoomCid?.let { ZoomableImageDialog(it) { zoomCid = null } }
}

/** Swipeable carousel for a post's media. Single item shows plain; multiple get a
 *  pager with a counter + dots. Tapping a photo opens the pinch-zoom viewer. */
@Composable
private fun MediaCarousel(media: List<Media>, onOpenPhoto: (String) -> Unit) {
    if (media.size == 1) { MediaItem(media[0], onOpenPhoto); return }
    val pager = rememberPagerState(pageCount = { media.size })
    Box {
        HorizontalPager(state = pager, modifier = Modifier.fillMaxWidth()) { page ->
            MediaItem(media[page], onOpenPhoto)
        }
        Box(Modifier.align(Alignment.TopEnd).padding(8.dp).clip(RoundedCornerShape(10.dp)).background(Color.Black.copy(alpha = 0.45f)).padding(8.dp, 3.dp)) {
            Text("${pager.currentPage + 1}/${media.size}", color = Color.White, fontSize = 11.sp)
        }
        Row(Modifier.align(Alignment.BottomCenter).padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            repeat(media.size) { i ->
                Box(Modifier.size(6.dp).clip(CircleShape).background(if (i == pager.currentPage) Gold else Color.White.copy(alpha = 0.55f)))
            }
        }
    }
}

@Composable
private fun MediaItem(m: Media, onOpenPhoto: (String) -> Unit) {
    if (m.type == "video") VideoTile(m.cid)
    else {
        val uri = HeyApi.mediaUri(m.cid)
        // Fixed 4:5 window — stable height in the scrolling feed (no post-load resize jank) and a
        // consistent rhythm. The photo is shown FULL (Fit, never cropped or distorted); a blurred,
        // cropped copy of the SAME photo fills the gaps, so portrait AND landscape look clean with
        // no black bars. (Modifier.blur is hardware on API 31+, a sharp fill on older — still no black.)
        Box(
            Modifier.fillMaxWidth().aspectRatio(4f / 5f)
                .clip(RoundedCornerShape(14.dp))
                .clickable { onOpenPhoto(m.cid) }
        ) {
            AsyncImage(
                model = uri, contentDescription = null, contentScale = ContentScale.Crop,
                modifier = Modifier.matchParentSize().blur(26.dp),
            )
            // Faint scrim so the sharp photo's edges read against its own blurred backdrop.
            Box(Modifier.matchParentSize().background(Color.Black.copy(alpha = 0.12f)))
            AsyncImage(
                model = uri, contentDescription = null, contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** Full-screen pinch-to-zoom + pan viewer for a posted photo. */
@Composable
private fun ZoomableImageDialog(cid: String, onClose: () -> Unit) {
    androidx.compose.ui.window.Dialog(
        onDismissRequest = onClose,
        properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)
    ) {
        var scale by remember { mutableStateOf(1f) }
        var offset by remember { mutableStateOf(Offset.Zero) }
        val state = rememberTransformableState { zoomChange, panChange, _ ->
            scale = (scale * zoomChange).coerceIn(1f, 5f)
            offset = if (scale > 1f) offset + panChange else Offset.Zero
        }
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.96f)), Alignment.Center) {
            AsyncImage(
                model = HeyApi.mediaUri(cid), contentDescription = null, contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize()
                    .graphicsLayer(scaleX = scale, scaleY = scale, translationX = offset.x, translationY = offset.y)
                    .transformable(state)
            )
            IconButton(onClick = onClose, modifier = Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(8.dp)) {
                Icon(Icons.Filled.Close, "Close", tint = Color.White)
            }
            Text("Pinch to zoom", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp,
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(16.dp))
        }
    }
}

@Composable
private fun CommentRow(c: Comment, indent: Boolean, onReply: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = if (indent) 26.dp else 0.dp, top = 3.dp), verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f)) {
            Row {
                Text((c.authorName.ifBlank { HeyApi.shortDid(c.author) }) + "  ", color = goldInk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                Text(c.text, color = ink, fontSize = 14.sp)
            }
        }
        if (!indent) TextButton(onClick = onReply, contentPadding = PaddingValues(6.dp, 0.dp)) { Text("Reply", color = muted, fontSize = 11.sp) }
    }
}

// ── composer ─────────────────────────────────────────────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ComposerScreen(onBack: () -> Unit, onPosted: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var caption by remember { mutableStateOf("") }
    var picked by remember { mutableStateOf<List<PickedMedia>>(emptyList()) }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }

    // Pick up to 10 photos/videos at once; images shrink to WebP. Re-picking
    // appends (capped at 10) so "add more" keeps the earlier selection.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(10)) { uris ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            status = "Reading…"
            val room = 10 - picked.size
            val added = withContext(Dispatchers.IO) {
                uris.take(room).mapNotNull { uri ->
                    runCatching {
                        val mime = ctx.contentResolver.getType(uri) ?: "image/*"
                        val video = mime.startsWith("video/")
                        val raw = ctx.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
                        val bytes = if (video) raw else scaleWebp(raw)
                        PickedMedia(bytes, if (video) mime else "image/webp", video,
                            if (video) null else BitmapFactory.decodeByteArray(bytes, 0, bytes.size))
                    }.getOrNull()
                }
            }
            picked = (picked + added).take(10)
            status = if (picked.isEmpty()) "Could not read those files" else "${picked.size}/10 selected"
        }
    }
    fun launchPicker() = picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))

    ModalBottomSheet(onDismissRequest = { if (!busy) onBack() }, containerColor = sheetBg) {
        // animateContentSize so adding/removing a photo grows the sheet smoothly instead of jumping.
        Column(Modifier.fillMaxWidth().padding(20.dp, 4.dp, 20.dp, 8.dp).animateContentSize().verticalScroll(rememberScrollState())) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.AutoAwesome, null, tint = goldInk, modifier = Modifier.size(22.dp))
                Spacer(Modifier.width(8.dp))
                Text("Share a moment", color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            }
            Spacer(Modifier.height(4.dp))
            Text("Add a few photos or a video, then a caption.", color = muted, fontSize = 13.sp)
            Spacer(Modifier.height(16.dp))
            if (picked.isEmpty()) {
                // Tap the box itself to pick — no separate button.
                Box(Modifier.fillMaxWidth().height(170.dp).glass(16.dp).clickable(enabled = !busy) { launchPicker() }, Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(Icons.Filled.AddPhotoAlternate, null, tint = goldInk, modifier = Modifier.size(44.dp))
                        Spacer(Modifier.height(8.dp))
                        Text("Tap to add photos or video", color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                        Text("Up to 10 — they'll stack up here", color = muted, fontSize = 12.sp)
                    }
                }
            } else {
                PolaroidStack(
                    picked = picked,
                    canAdd = picked.size < 10 && !busy,
                    onRemove = { i -> picked = picked.toMutableList().also { if (i < it.size) it.removeAt(i) } },
                    onAdd = { launchPicker() },
                )
                Spacer(Modifier.height(8.dp))
                Text("${picked.size}/10 · tap ✕ to remove", color = muted, fontSize = 12.sp, modifier = Modifier.align(Alignment.CenterHorizontally))
            }
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = caption, onValueChange = { caption = it },
                placeholder = { Text("Write a caption…", color = muted) },
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink),
            )
            Spacer(Modifier.height(18.dp))
            Button(
                onClick = {
                    if (picked.isEmpty()) { status = "Add a photo or video first"; return@Button }
                    busy = true; status = "Publishing…"
                    val items = picked
                    scope.launch {
                        val ok = withContext(Dispatchers.IO) {
                            runCatching {
                                val tiles = items.mapIndexed { i, pm ->
                                    val fname = if (pm.isVideo) "video$i.mp4" else "photo$i.webp"
                                    val tile = HeyApi.uploadMedia(pm.bytes, pm.mime, fname)
                                    if (tile.has("error")) error(tile.getString("error"))
                                    tile
                                }
                                val post = HeyApi.createPost(caption, tiles)
                                if (post.has("error")) error(post.getString("error")); true
                            }.getOrElse { status = "Failed: ${it.message}"; false }
                        }
                        busy = false; if (ok) onPosted()
                    }
                },
                enabled = !busy && picked.isNotEmpty(), modifier = Modifier.fillMaxWidth().height(52.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
            ) {
                if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                else Text("Share the moment", fontWeight = FontWeight.Bold)
            }
            if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = muted, fontSize = 13.sp) }
            Spacer(Modifier.height(20.dp))
            Text("Pinned on-device · signed · federated via Carrier", color = muted, fontSize = 11.sp, modifier = Modifier.align(Alignment.CenterHorizontally))
        }
    }
}

private data class PickedMedia(val bytes: ByteArray, val mime: String, val isVideo: Boolean, val preview: Bitmap?)

/** A playful fanned stack of polaroid-style cards for the chosen photos —
 *  mobile-friendly, scrollable, each card tilted + tap-✕ to remove. */
@Composable
private fun PolaroidStack(picked: List<PickedMedia>, canAdd: Boolean, onRemove: (Int) -> Unit, onAdd: () -> Unit) {
    val tilts = listOf(-5f, 4f, -3f, 5f, -2f, 3f)
    LazyRow(
        Modifier.fillMaxWidth().height(184.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 10.dp, end = 10.dp, top = 16.dp, bottom = 8.dp),
        horizontalArrangement = Arrangement.spacedBy((-16).dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        itemsIndexed(picked) { i, pm ->
            Box(Modifier.graphicsLayer { rotationZ = tilts[i % tilts.size] }) {
                // Polaroid: white frame, photo, extra bottom lip.
                Column(
                    Modifier.shadow(8.dp, RoundedCornerShape(12.dp)).clip(RoundedCornerShape(12.dp))
                        .background(Color.White).padding(7.dp, 7.dp, 7.dp, 16.dp)
                ) {
                    Box(Modifier.size(118.dp).clip(RoundedCornerShape(7.dp)).background(Color(0xFFE9E9EE)), Alignment.Center) {
                        if (pm.preview != null) Image(pm.preview.asImageBitmap(), null, Modifier.fillMaxSize().clip(RoundedCornerShape(7.dp)), contentScale = ContentScale.Crop)
                        else Icon(Icons.Filled.PlayCircle, null, tint = Navy, modifier = Modifier.size(42.dp))
                    }
                }
                // ✕ on the TOP-LEFT so the next (on-top) card never covers it.
                Box(
                    Modifier.align(Alignment.TopStart).offset(x = (-3).dp, y = (-3).dp).size(24.dp)
                        .clip(CircleShape).background(Navy.copy(alpha = 0.9f)).clickable { onRemove(i) },
                    Alignment.Center
                ) { Icon(Icons.Filled.Close, "Remove", tint = Color.White, modifier = Modifier.size(14.dp)) }
            }
        }
        if (canAdd) item {
            Box(Modifier.graphicsLayer { rotationZ = 3f }) {
                Column(
                    Modifier.shadow(4.dp, RoundedCornerShape(12.dp)).clip(RoundedCornerShape(12.dp))
                        .background(Color.White.copy(alpha = 0.12f)).border(1.5.dp, glassBorder, RoundedCornerShape(12.dp))
                        .clickable { onAdd() }.padding(7.dp, 7.dp, 7.dp, 16.dp)
                ) {
                    Box(Modifier.size(118.dp), Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Icon(Icons.Filled.AddPhotoAlternate, null, tint = goldInk, modifier = Modifier.size(34.dp))
                            Spacer(Modifier.height(4.dp))
                            Text("Add", color = muted, fontSize = 12.sp)
                        }
                    }
                }
            }
        }
    }
}

// ── profile + follow ─────────────────────────────────────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProfileScreen(did: String, online: Boolean, peers: Int, topPad: Dp = 12.dp, onOpenProfile: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var showQr by remember { mutableStateOf(false) }
    var showAdd by remember { mutableStateOf(false) }
    var showSettings by remember { mutableStateOf(false) }
    var showConn by remember { mutableStateOf(false) }
    var showAbout by remember { mutableStateOf(false) }
    var following by remember { mutableStateOf<List<Follow>>(emptyList()) }
    var followers by remember { mutableStateOf<List<Follow>>(emptyList()) }
    var contacts by remember { mutableStateOf(0) }
    var me by remember { mutableStateOf(Profile(did, "", "", "")) }
    var showEdit by remember { mutableStateOf(false) }
    fun reloadFollowing() {
        scope.launch {
            following = withContext(Dispatchers.IO) { runCatching { HeyApi.following() }.getOrDefault(emptyList()) }
            followers = withContext(Dispatchers.IO) { runCatching { HeyApi.followers() }.getOrDefault(emptyList()) }
            contacts = withContext(Dispatchers.IO) { runCatching { HeyApi.chats().size }.getOrDefault(0) }
            me = withContext(Dispatchers.IO) { runCatching { HeyApi.profile() }.getOrDefault(Profile(did, "", "", "")) }
        }
    }
    LaunchedEffect(Unit) { reloadFollowing() }

    // padding AFTER verticalScroll → content scrolls BEHIND the frosted top bar (milky like Feed),
    // not parked below it.
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = topPad, bottom = 20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            IconButton(onClick = { showSettings = true }) { Icon(Icons.Filled.Settings, "Settings", tint = ink) }
        }
        Avatar(me.avatar, did, 88)
        Spacer(Modifier.height(14.dp))
        Text(me.nickname.ifBlank { "You" }, color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        if (me.bio.isNotBlank()) {
            Spacer(Modifier.height(4.dp))
            Text(me.bio, color = muted, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        }
        Spacer(Modifier.height(6.dp))
        Text(if (online) "Carrier online · $peers peers" else "Carrier connecting…", color = if (online) good else goldInk, fontSize = 13.sp)
        Spacer(Modifier.height(10.dp))
        OutlinedButton(onClick = { showEdit = true }) {
            Icon(Icons.Filled.Edit, null, Modifier.size(16.dp)); Spacer(Modifier.width(6.dp)); Text("Edit profile", color = ink)
        }

        Spacer(Modifier.height(14.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Badge2(if (online) "● Online" else "○ Offline", if (online) good else muted)
            Badge2("${followers.size} followers", goldInk)
            Badge2("${following.size} following", ink)
            Badge2("$contacts chats", ink)
        }

        Spacer(Modifier.height(14.dp))
        // security card
        Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.VerifiedUser, null, tint = good, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text("Security", color = ink, fontWeight = FontWeight.SemiBold)
            }
            Spacer(Modifier.height(8.dp))
            SecRow("Encryption", "End-to-end · ML-KEM-768 + X25519")
            SecRow("Keys", "Held on this device, never uploaded")
            SecRow("Identity", "Self-sovereign did:key — owned by you")
        }

        Spacer(Modifier.height(14.dp))
        // connection explainer (tap to open)
        Column(Modifier.fillMaxWidth().glass().clickable { showConn = true }.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.Hub, null, tint = goldInk, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text("Connection", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                Text(if (online) "● Live" else "○ Connecting", color = if (online) good else muted, fontSize = 12.sp)
                Spacer(Modifier.width(6.dp))
                Icon(Icons.Filled.ChevronRight, null, tint = muted)
            }
            Spacer(Modifier.height(6.dp))
            Text("See how your device connects — what the relay does and how data flows peer-to-peer.", color = muted, fontSize = 13.sp)
        }

        Spacer(Modifier.height(14.dp))
        BatteryCard()

        Spacer(Modifier.height(14.dp))
        // about (transparency: what Elastos tech does for you)
        Row(Modifier.fillMaxWidth().glass().clickable { showAbout = true }.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Info, null, tint = goldInk, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text("About Hey", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            Icon(Icons.Filled.ChevronRight, null, tint = muted)
        }

        Spacer(Modifier.height(14.dp))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text("Appearance", color = muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
            ThemeToggle()
        }

        Spacer(Modifier.height(14.dp))
        // Connecting is link/QR only — a bare DID can't open a private channel.
        Button(onClick = { showAdd = true }, modifier = Modifier.fillMaxWidth().height(50.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
            Icon(Icons.Filled.PersonAdd, null); Spacer(Modifier.width(8.dp)); Text("Add / follow someone", fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(8.dp))
        OutlinedButton(onClick = { showQr = true }, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Filled.QrCode2, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Share my invite QR", color = ink)
        }

        if (followers.isNotEmpty()) {
            Spacer(Modifier.height(18.dp))
            Text("Followers (${followers.size})", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.align(Alignment.Start))
            Spacer(Modifier.height(8.dp))
            followers.forEach { f -> PersonRow(f.did, onClick = { onOpenProfile(f.did) }) }
        }

        Spacer(Modifier.height(18.dp))
        Text("Following (${following.size})", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.align(Alignment.Start))
        Spacer(Modifier.height(8.dp))
        following.forEach { f ->
            PersonRow(f.did, onClick = { onOpenProfile(f.did) }, trailing = {
                TextButton(onClick = {
                    scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.unfollow(f.did) } }; reloadFollowing() }
                }) { Text("Unfollow", color = muted, fontSize = 12.sp) }
            })
        }
        Spacer(Modifier.height(96.dp))
    }

    if (showQr) MyQrSheet(did) { showQr = false }
    if (showAdd) AddFriendSheet(onClose = { showAdd = false }, onFollowed = { showAdd = false; reloadFollowing() })
    if (showEdit) EditProfileSheet(me, onClose = { showEdit = false }, onSaved = { showEdit = false; reloadFollowing() })
    if (showSettings) SettingsSheet(did, onClose = { showSettings = false },
        onShowQr = { showSettings = false; showQr = true },
        onShowConnection = { showSettings = false; showConn = true })
    if (showConn) ConnectionSheet(online, peers) { showConn = false }
    if (showAbout) AboutSheet { showAbout = false }
}

/** Profile settings (the gear): your DID lives here, demoted out of the main
 *  profile so people connect via invite/QR — not by pasting a DID. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsSheet(did: String, onClose: () -> Unit, onShowQr: () -> Unit, onShowConnection: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    // Recovery phrase is revealed ONLY after a fresh biometric/TEE check (below).
    var showPhrase by remember { mutableStateOf<String?>(null) }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("Settings", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(16.dp))

            Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Badge, null, tint = goldInk, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Your identity", color = ink, fontWeight = FontWeight.SemiBold)
                }
                Spacer(Modifier.height(8.dp))
                Text(did, color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(10.dp))
                Row {
                    FilledTonalButton(onClick = { clipboard.setText(AnnotatedString(did)) }) {
                        Icon(Icons.Filled.ContentCopy, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Copy DID")
                    }
                    Spacer(Modifier.width(10.dp))
                    FilledTonalButton(onClick = onShowQr) {
                        Icon(Icons.Filled.QrCode2, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("My QR")
                    }
                }
                Spacer(Modifier.height(8.dp))
                Text("This DID is your sovereign identity — it signs everything you create. To connect with someone, share your invite link or QR; a DID alone can't open a private channel.", color = muted, fontSize = 12.sp)
            }

            Spacer(Modifier.height(12.dp))
            Row(Modifier.fillMaxWidth().glass().clickable { onShowConnection() }.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.Hub, null, tint = goldInk, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(8.dp))
                Text("How Hey connects", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                Icon(Icons.Filled.ChevronRight, null, tint = muted)
            }

            Spacer(Modifier.height(12.dp))
            // Priority 1: App lock toggle — switch require-unlock mode on/off later.
            // ON  → resolve the phrase → skippable RecordPhraseGate → enableVault (atomic
            //       seal). This routes the ON direction through the phrase gate (fixes the
            //       review's HIGH H2.2-1: the switch must NEVER seal bare).
            // OFF → disableVault (KEEPS its atomic persistIdentity-before-clear ordering).
            val scope = rememberCoroutineScope()
            val activity = ctx as? androidx.fragment.app.FragmentActivity
            val lockable = IdentityVault.available(ctx) && activity != null
            var on by remember { mutableStateOf(IdentityVault.isOn(ctx)) }
            var busy by remember { mutableStateOf(false) }
            // The phrase the user must record before the ON-direction seal; null = no gate.
            var lockGatePhrase by remember { mutableStateOf<String?>(null) }
            Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Lock, null, tint = if (on) good else goldInk, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(8.dp))
                    Column(Modifier.weight(1f)) {
                        Text("App lock", color = ink, fontWeight = FontWeight.SemiBold)
                        Text(if (lockable) "Require a fingerprint/face each time you open Hey; your seed is sealed in the Titan M / Knox Vault / TEE" else "No biometric set up on this device", color = muted, fontSize = 12.sp)
                    }
                    if (busy) CircularProgressIndicator(Modifier.size(22.dp), color = goldInk, strokeWidth = 2.dp)
                    else Switch(checked = on, enabled = lockable, onCheckedChange = { want ->
                        if (want) {
                            // ON: resolve the phrase, then show the (skippable) phrase gate.
                            // The gate's onConfirmed performs the atomic enableVault seal.
                            busy = true
                            scope.launch {
                                // H5: when the spend/reveal binding is enrolled the bare
                                // recoveryPhrase() JNI refuses — use the verified reveal.
                                val phrase = HeyApi.unlockedSeed
                                    ?: (if (SpendAuth.isEnrolled(ctx)) SpendAuth.revealSeed(activity) else HeyApi.recoveryPhrase())
                                if (phrase.isNullOrBlank()) {
                                    busy = false
                                    android.widget.Toast.makeText(ctx, "Couldn't read your recovery phrase — try again", android.widget.Toast.LENGTH_SHORT).show()
                                } else lockGatePhrase = phrase
                            }
                        } else {
                            busy = true
                            disableVault(activity!!, ctx, scope) { ok ->
                                on = !ok; busy = false
                                if (!ok) android.widget.Toast.makeText(ctx, "Couldn't turn off — try again", android.widget.Toast.LENGTH_SHORT).show()
                            }
                        }
                    }, colors = SwitchDefaults.colors(checkedThumbColor = Navy, checkedTrackColor = Gold))
                }
                // ON-direction phrase gate (skippable). On confirm → atomic enableVault.
                lockGatePhrase?.let { phrase ->
                    if (activity != null) RecordPhraseGate(
                        phrase = phrase,
                        onConfirmed = {
                            lockGatePhrase = null
                            enableVault(activity, ctx, scope) { ok ->
                                on = ok; busy = false
                                android.widget.Toast.makeText(ctx, if (ok) "App lock on — keys sealed in hardware" else "Couldn't enable — try again", android.widget.Toast.LENGTH_SHORT).show()
                            }
                        },
                        onCancel = { lockGatePhrase = null; busy = false }, // not sealed; switch stays off
                    )
                }
                Spacer(Modifier.height(6.dp))
                Text(if (on) "App lock ON: a fingerprint/face opens Hey, and your keys are sealed in hardware — never stored in plaintext. Messages keep arriving in the background and decrypt the moment you unlock."
                     else if (lockable) "App lock OFF (open freely): Hey opens with no biometric. Your keys are protected by Android's at-rest encryption + sandbox, and recoverable by malware only on an unlocked, rooted phone. Turn on to seal your keys behind a fingerprint."
                     else "Keys are protected by Android's at-rest encryption + sandbox. Set a screen lock to also seal them behind a fingerprint.", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(10.dp))
                OutlinedButton(onClick = {
                    // H5: when the hardware spend/reveal binding is enrolled, the seed
                    // reveal is bound to a fresh Keystore signature the Rust guard verifies
                    // (hey_recovery_phrase_hw) — the bare JNI now refuses while binding is
                    // active. Otherwise fall back to the legacy biometric gate.
                    if (activity != null && SpendAuth.isEnrolled(ctx)) {
                        scope.launch {
                            val phrase = SpendAuth.revealSeed(activity)
                            if (phrase.isNullOrBlank()) android.widget.Toast.makeText(ctx, "Couldn't verify — try again", android.widget.Toast.LENGTH_SHORT).show()
                            else showPhrase = phrase
                        }
                    } else {
                        val reveal = {
                            val phrase = identityBackup(ctx)
                            if (phrase.isNullOrBlank()) android.widget.Toast.makeText(ctx, "No identity to back up yet", android.widget.Toast.LENGTH_SHORT).show()
                            else showPhrase = phrase
                        }
                        // Require a fresh fingerprint/face/device-credential (TEE-backed)
                        // before the seed is ever shown. Old phones w/o biometrics: reveal directly.
                        requireAuth(activity, ctx) { reveal() }
                    }
                }) { Icon(Icons.Filled.Key, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Reveal recovery phrase", color = ink) }

                // Hardware spend confirmation — per-transfer StrongBox/TEE biometric.
                if (SpendAuth.available(ctx) && activity != null) {
                    Spacer(Modifier.height(14.dp))
                    var spendOn by remember { mutableStateOf(SpendAuth.isEnrolled(ctx)) }
                    var spendBusy by remember { mutableStateOf(false) }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Fingerprint, null, tint = if (spendOn) good else goldInk, modifier = Modifier.size(20.dp))
                        Spacer(Modifier.width(8.dp))
                        Column(Modifier.weight(1f)) {
                            Text("Hardware spend confirmation", color = ink, fontWeight = FontWeight.SemiBold)
                            Text("Every transfer needs a fresh fingerprint signed in StrongBox/TEE — stops an in-process attacker from spending your funds", color = muted, fontSize = 12.sp)
                        }
                        if (spendBusy) CircularProgressIndicator(Modifier.size(22.dp), color = goldInk, strokeWidth = 2.dp)
                        else Switch(checked = spendOn, onCheckedChange = { want ->
                            if (want) {
                                spendBusy = true
                                SpendAuth.enroll(activity) { ok ->
                                    spendOn = ok; spendBusy = false
                                    android.widget.Toast.makeText(ctx, if (ok) "Hardware spend confirmation on" else "Couldn't enroll — no biometric or hardware key", android.widget.Toast.LENGTH_SHORT).show()
                                }
                            } else {
                                // H4: turning it OFF needs a fresh hardware signature (non-disarmable).
                                spendBusy = true
                                SpendAuth.unenroll(activity) { ok ->
                                    spendOn = !ok; spendBusy = false
                                    if (!ok) android.widget.Toast.makeText(ctx, "Couldn't verify — hardware confirmation stays on", android.widget.Toast.LENGTH_SHORT).show()
                                }
                            }
                        }, colors = SwitchDefaults.colors(checkedThumbColor = Navy, checkedTrackColor = Gold))
                    }
                }
            }

            showPhrase?.let { phrase ->
                AlertDialog(
                    onDismissRequest = { showPhrase = null },
                    icon = { Icon(Icons.Filled.Key, null, tint = goldInk) },
                    title = { Text("Your recovery phrase", color = ink) },
                    text = {
                        Column {
                            SecureWindow() // block screenshots / recents while the phrase is shown
                            Text("These 12 words ARE your account — they recover your Hey identity, your Elastos DID, and your wallets (here or in official Elastos Essentials). Anyone with them controls everything. Write them down offline; never share or screenshot.",
                                color = muted, fontSize = 13.sp, lineHeight = 19.sp)
                            Spacer(Modifier.height(14.dp))
                            Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Color.Black.copy(alpha = 0.13f)).padding(14.dp)) {
                                Text(phrase, color = ink, fontSize = 16.sp, fontFamily = mono, lineHeight = 26.sp)
                            }
                        }
                    },
                    confirmButton = {
                        TextButton(onClick = {
                            // Mark the clip SENSITIVE (hidden from the clipboard preview + OEM/IME
                            // cloud-sync) and best-effort auto-clear after 60s, instead of leaving the
                            // recovery phrase in clipboard history indefinitely.
                            val cm = ctx.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                            val clip = android.content.ClipData.newPlainText("Hey recovery phrase", phrase)
                            if (Build.VERSION.SDK_INT >= 33) {
                                clip.description.extras = android.os.PersistableBundle().apply {
                                    putBoolean(android.content.ClipDescription.EXTRA_IS_SENSITIVE, true)
                                }
                            }
                            cm.setPrimaryClip(clip)
                            android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                                runCatching {
                                    if (cm.primaryClip?.getItemAt(0)?.text?.toString() == phrase) {
                                        if (Build.VERSION.SDK_INT >= 28) cm.clearPrimaryClip()
                                        else cm.setPrimaryClip(android.content.ClipData.newPlainText("", ""))
                                    }
                                }
                            }, 15_000)
                            android.widget.Toast.makeText(ctx, "Copied — clears in 15s. Store it offline, secretly.", android.widget.Toast.LENGTH_LONG).show()
                        }) { Text("Copy", color = goldInk, fontWeight = FontWeight.Bold) }
                    },
                    dismissButton = { TextButton(onClick = { showPhrase = null }) { Text("Done", color = muted) } },
                    containerColor = sheetBg,
                )
            }

            Spacer(Modifier.height(12.dp))
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("Appearance", color = muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
                ThemeToggle()
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** Illustrated explainer: relay = matchmaker, carrier = direct E2E pipe. Shows
 *  the live mode (direct vs relay-assisted) read from the runtime status. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ConnectionSheet(online: Boolean, peers: Int, onClose: () -> Unit) {
    val ctx = LocalContext.current
    var direct by remember { mutableStateOf(false) }
    var netLabel by remember { mutableStateOf("") }
    var v6 by remember { mutableStateOf(false) }
    var pubV4 by remember { mutableStateOf("") }
    var pubV6 by remember { mutableStateOf("") }
    var udpV4 by remember { mutableStateOf(false) }
    var udpV6 by remember { mutableStateOf(false) }
    var localAddrs by remember { mutableStateOf<List<String>>(emptyList()) }
    var directPeers by remember { mutableStateOf(0) }
    var relayPeers by remember { mutableStateOf(0) }
    // Relay choice: "" = standard (iroh), the federation URL, or a custom URL.
    val storedRelay = remember { HeyApi.customRelay(ctx) }
    var relayChoice by remember {
        mutableStateOf(
            when {
                // blank = the community default (elastos.app + n0); the elastos.app
                // URL itself also maps to the community choice
                storedRelay.isBlank() -> "community"
                storedRelay.equals(HeyApi.RELAY_FEDERATED_URL, ignoreCase = true) -> "community"
                else -> "custom"
            }
        )
    }
    var relayInput by remember { mutableStateOf(if (storedRelay.equals(HeyApi.RELAY_FEDERATED_URL, ignoreCase = true)) "" else storedRelay) }
    var relaySaved by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        while (true) {
            val h = withContext(Dispatchers.IO) { runCatching { HeyApi.health() }.getOrNull() }
            if (h != null) {
                direct = h.optBoolean("direct")
                val hasV4 = h.optBoolean("ipv4"); v6 = h.optBoolean("ipv6_global")
                pubV4 = h.optString("public_v4").takeIf { it.isNotBlank() && it != "null" } ?: ""
                pubV6 = h.optString("public_v6").takeIf { it.isNotBlank() && it != "null" } ?: ""
                udpV4 = h.optBoolean("udp_v4"); udpV6 = h.optBoolean("udp_v6")
                directPeers = h.optInt("direct_peers"); relayPeers = h.optInt("relay_peers")
                localAddrs = h.optJSONArray("local_addrs")?.let { arr -> (0 until arr.length()).map { arr.optString(it) }.filter { it.isNotBlank() } } ?: emptyList()
                netLabel = when {
                    v6 && hasV4 -> "IPv6 + IPv4"
                    v6 -> "IPv6 (global)"
                    hasV4 -> "IPv4 (behind NAT)"
                    else -> "—"
                }
            }
            kotlinx.coroutines.delay(2000)
        }
    }
    val gc = goldInk; val mc = muted; val gd = good
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("How Hey connects", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(2.dp))
            Text("No servers store your data. Your device is the node.", color = muted, fontSize = 13.sp)
            Spacer(Modifier.height(16.dp))

            // Diagram: relay introduces (dashed), devices talk directly (solid).
            Box(Modifier.fillMaxWidth().height(150.dp).glass().padding(8.dp)) {
                Canvas(Modifier.fillMaxSize()) {
                    val you = Offset(size.width * 0.15f, size.height * 0.80f)
                    val friend = Offset(size.width * 0.85f, size.height * 0.80f)
                    val relay = Offset(size.width * 0.50f, size.height * 0.18f)
                    val dash = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(12f, 12f))
                    drawLine(mc.copy(alpha = 0.6f), you, relay, strokeWidth = 3f, pathEffect = dash)
                    drawLine(mc.copy(alpha = 0.6f), friend, relay, strokeWidth = 3f, pathEffect = dash)
                    drawLine(if (direct) gc else mc.copy(alpha = 0.5f), you, friend, strokeWidth = 7f)
                }
                ChipLabel("Relay", Icons.Filled.Hub, androidx.compose.ui.BiasAlignment(0f, -0.75f))
                ChipLabel("You", Icons.Filled.Smartphone, androidx.compose.ui.BiasAlignment(-0.72f, 0.65f))
                ChipLabel("Friend", Icons.Filled.Smartphone, androidx.compose.ui.BiasAlignment(0.72f, 0.65f))
                Box(Modifier.align(androidx.compose.ui.BiasAlignment(0f, 0.62f)).clip(RoundedCornerShape(8.dp)).background(sheetBg).padding(6.dp, 1.dp)) {
                    Text(if (direct) "direct · encrypted" else "relayed · encrypted", color = if (direct) gd else mc, fontSize = 10.sp)
                }
            }

            Spacer(Modifier.height(14.dp))
            ConnStep(Icons.Filled.Hub, "Relay introduces", "The relay finds your friend's device and helps the two punch through firewalls/NAT. It's a matchmaker — it never stores your account or messages.")
            ConnStep(Icons.Filled.SwapHoriz, "Carrier connects", "Your two devices form a direct peer-to-peer link — the Carrier (iroh). Once joined, messages and media flow device-to-device.")
            ConnStep(Icons.Filled.Lock, "End-to-end encrypted", "Everything is sealed with ML-KEM-768 + X25519. Even when traffic must pass a relay, it only ever sees ciphertext — never your content.")

            Spacer(Modifier.height(14.dp))
            Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(9.dp).clip(androidx.compose.foundation.shape.CircleShape).background(if (online) good else muted))
                    Spacer(Modifier.width(8.dp))
                    Text(if (online) "Live on the carrier · $peers connected" else "Connecting to the carrier…", color = if (online) good else goldInk, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                }
                Spacer(Modifier.height(6.dp))
                Text(
                    if (direct) "Direct mode: data is travelling peer-to-peer. The relay is only introducing devices."
                    else "Relay-assisted: this network blocks direct connections, so encrypted data currently rides the relay. It stays end-to-end encrypted, and Hey keeps trying to upgrade to a direct link.",
                    color = muted, fontSize = 12.sp,
                )
                if (netLabel.isNotBlank()) {
                    Spacer(Modifier.height(10.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(if (v6) Icons.Filled.Bolt else Icons.Filled.Lan, null, tint = if (v6) good else muted, modifier = Modifier.size(15.dp))
                        Spacer(Modifier.width(6.dp))
                        Text("Network: $netLabel", color = ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                    }
                    Spacer(Modifier.height(3.dp))
                    Text(
                        if (v6) "A global IPv6 address lets your phone reach others directly — best for direct mode."
                        else "IPv4 behind NAT/CGNAT can't be reached directly on mobile, so it stays relay-assisted. Switch to an IPv6 network for direct.",
                        color = muted, fontSize = 11.sp,
                    )
                    // The device's actual PUBLIC addresses (what the world sees) — observed via the relay.
                    if (pubV6.isNotBlank() || pubV4.isNotBlank()) {
                        Spacer(Modifier.height(10.dp))
                        if (pubV6.isNotBlank()) {
                            PublicAddrRow("Public IPv6", pubV6, good)
                            Spacer(Modifier.height(4.dp))
                        }
                        if (pubV4.isNotBlank()) PublicAddrRow("Public IPv4", pubV4, goldInk)
                    }
                    // ── direct/relay breakdown + the agnostic-discovery proof + which address Hey binds ──
                    Spacer(Modifier.height(10.dp))
                    Box(Modifier.fillMaxWidth().height(1.dp).background(glassBorder))
                    Spacer(Modifier.height(10.dp))
                    Text("Live links: $directPeers direct · $relayPeers relayed", color = ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                    Spacer(Modifier.height(8.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(8.dp).clip(androidx.compose.foundation.shape.CircleShape).background(if (udpV4 || udpV6) good else muted))
                        Spacer(Modifier.width(7.dp))
                        Text("Direct UDP path: " + listOfNotNull(if (udpV4) "IPv4" else null, if (udpV6) "IPv6" else null).joinToString(" + ").ifBlank { "none yet — relay only" }, color = muted, fontSize = 11.sp)
                    }
                    if (localAddrs.isNotEmpty()) {
                        Spacer(Modifier.height(10.dp))
                        Text("Address Hey is using", color = ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        Spacer(Modifier.height(3.dp))
                        localAddrs.forEach { a ->
                            Text("• $a", color = muted, fontSize = 11.sp, fontFamily = mono)
                        }
                        Spacer(Modifier.height(4.dp))
                        Text("That's the interface Hey binds: on a VPN you'll see the tunnel address (10.x); on plain WiFi your LAN address (192.168.x). The Public IP above is your egress — it becomes your VPN server's IP when the tunnel carries Hey, or your router's IP when it doesn't.", color = muted, fontSize = 10.sp, lineHeight = 14.sp)
                    }
                }
            }

            // ── Relay / "Hey mesh hub" selection ──
            Spacer(Modifier.height(14.dp))
            Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Hub, null, tint = goldInk, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Relay server", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                }
                Spacer(Modifier.height(4.dp))
                Text(
                    "The relay only introduces peers + carries encrypted data when a direct link isn't possible. Friends on a different relay still reach you — every device is reachable through its own.",
                    color = muted, fontSize = 11.sp,
                )
                Spacer(Modifier.height(10.dp))
                ModeOption(
                    "Community relay — Elastos.app (recommended)", "The Elastos community federation relay, with iroh's network as automatic backup. Zero setup.",
                    "community", relayChoice,
                ) { relayChoice = "community"; HeyApi.setCustomRelay(ctx, ""); relaySaved = true }
                Spacer(Modifier.height(8.dp))
                ModeOption(
                    "My own relay", "Self-hosted hub: paste your relay's address. Nothing about your device touches a third party.",
                    "custom", relayChoice,
                ) { relayChoice = "custom"; relaySaved = false }
                if (relayChoice == "custom") {
                    Spacer(Modifier.height(10.dp))
                    OutlinedTextField(
                        value = relayInput, onValueChange = { relayInput = it; relaySaved = false }, singleLine = true,
                        label = { Text("Your Hey relay (https://host:port)") },
                        placeholder = { Text("https://relay.example.com:8443", color = muted) },
                        textStyle = androidx.compose.ui.text.TextStyle(fontFamily = mono, fontSize = 12.sp),
                        modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                    )
                    Spacer(Modifier.height(10.dp))
                    Button(
                        onClick = { if (relayInput.isNotBlank()) { HeyApi.setCustomRelay(ctx, relayInput); relaySaved = true } },
                        colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text("Save", fontWeight = FontWeight.SemiBold) }
                }
                if (relaySaved) {
                    Spacer(Modifier.height(8.dp))
                    Text("Saved ✓  Fully close + reopen Hey to apply.", color = good, fontSize = 11.sp)
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** Transparency: what the Elastos Internet OS stack + hey-core actually do for
 *  the user, in plain language, so the empowerment isn't a black box. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AboutSheet(onClose: () -> Unit) {
    var online by remember { mutableStateOf(false) }
    var direct by remember { mutableStateOf(false) }
    var peers by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) {
        while (true) {
            withContext(Dispatchers.IO) {
                runCatching { val h = HeyApi.health(); online = h.optBoolean("online"); direct = h.optBoolean("direct"); peers = h.optInt("peer_count") }
            }
            kotlinx.coroutines.delay(2000)
        }
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("About Hey", color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(2.dp))
            Text("Built on the Elastos Internet OS — you own your identity, your data, and your connections.", color = muted, fontSize = 13.sp)
            Spacer(Modifier.height(16.dp))

            AboutItem(Icons.Filled.Hub, "It runs on your phone", "There is no Hey server. A mini Elastos runtime + the Carrier (the peer-to-peer network) run inside the app, on your device. Your phone is the node — it holds your keys, signs your posts, stores your data, and talks straight to your friends' phones.")
            AboutItem(Icons.Filled.Badge, "You own your identity", "Your identity is a self-sovereign did:key — a keypair only your device holds. No email, no phone number, no account on someone's server. It signs everything you create so others can verify it's really you.")
            AboutItem(Icons.Filled.Lock, "Private by cryptography", "Messages and media are end-to-end encrypted with post-quantum crypto (ML-KEM-768 + X25519, ChaCha20-Poly1305). Even relays only ever see ciphertext — never your content.")
            // Live network mode — reflects how THIS phone is connected right now.
            AboutItemLive(
                Icons.Filled.SwapHoriz, "Peer-to-peer delivery",
                when {
                    !online -> "Connecting to the carrier…"
                    direct -> "Right now your phone is connected DIRECTLY — data flows device-to-device and the relay is only used to introduce peers."
                    else -> "Right now data rides the encrypted relay (this network blocks a direct link). It stays end-to-end encrypted, and Hey keeps trying to upgrade to a direct link."
                },
                chip = when {
                    !online -> "○ Connecting"
                    direct -> "● Direct P2P"
                    else -> "● Relay-assisted"
                },
                chipColor = when {
                    !online -> muted
                    direct -> good
                    else -> goldInk
                },
            )
            // Per-contact transport — who you're reaching directly vs over the relay.
            ContactsTransportSection()
            AboutItem(Icons.Filled.Shield, "Sandboxed & on-device", "All your keys and data live in Hey's private app storage, sandboxed by Android so other apps can't read them. Nothing is uploaded to a company. Hardware-backed encryption (StrongBox) and an optional biometric lock add another layer.")
            AboutItem(Icons.Filled.Public, "No lock-in", "hey-core is the same engine across phone, web and desktop, speaking open Elastos interfaces. Your identity and social graph are yours to take anywhere.")

            Spacer(Modifier.height(8.dp))
            Text("hey-core · Elastos Carrier (iroh) · IPFS content store · did:key identity", color = muted, fontSize = 11.sp)
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** Live per-contact transport roster: each 1:1 contact + whether you reach them
 *  DIRECT, over the RELAY, or they're OFFLINE right now. Refreshes every 3s. */
@Composable
private fun ContactsTransportSection() {
    var rows by remember { mutableStateOf<List<Pair<Chat, String>>>(emptyList()) }
    LaunchedEffect(Unit) {
        while (true) {
            rows = withContext(Dispatchers.IO) {
                runCatching {
                    HeyApi.chats().filter { !it.isGroup }.map { it to HeyApi.contactTransport(it.id) }
                }.getOrDefault(emptyList())
            }
            kotlinx.coroutines.delay(3000)
        }
    }
    if (rows.isEmpty()) return
    Spacer(Modifier.height(14.dp))
    Text("Your contacts", color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
    Spacer(Modifier.height(2.dp))
    Text("Who you're reaching directly vs over the relay, right now.", color = muted, fontSize = 12.sp)
    Spacer(Modifier.height(8.dp))
    rows.forEach { (chat, transport) ->
        val (dot, label) = when (transport) {
            "direct" -> good to "Direct P2P"
            "relay" -> goldInk to "Relay"
            else -> muted to "Offline"
        }
        Row(Modifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            Avatar(chat.avatar, chat.id, 28)
            Spacer(Modifier.width(10.dp))
            Text(chat.name, color = ink, fontSize = 14.sp, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("●", color = dot, fontSize = 10.sp)
            Spacer(Modifier.width(4.dp))
            Text(label, color = muted, fontSize = 12.sp)
        }
    }
}

/** Like AboutItem but with a live status chip (e.g. the current network mode). */
@Composable
private fun AboutItemLive(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, body: String, chip: String, chipColor: Color) {
    Row(Modifier.fillMaxWidth().padding(vertical = 7.dp)) {
        Box(Modifier.size(34.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
            Icon(icon, null, tint = Navy, modifier = Modifier.size(18.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(title, color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                Box(Modifier.clip(RoundedCornerShape(10.dp)).background(glassFill).border(1.dp, glassBorder, RoundedCornerShape(10.dp)).padding(8.dp, 2.dp)) {
                    Text(chip, color = chipColor, fontSize = 11.sp, fontWeight = FontWeight.Medium)
                }
            }
            Text(body, color = muted, fontSize = 13.sp)
        }
    }
}

@Composable
private fun AboutItem(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, body: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 7.dp)) {
        Box(Modifier.size(34.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
            Icon(icon, null, tint = Navy, modifier = Modifier.size(18.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text(body, color = muted, fontSize = 13.sp)
        }
    }
}

@Composable
private fun BoxScope.ChipLabel(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector, align: Alignment) {
    Row(Modifier.align(align).clip(RoundedCornerShape(12.dp)).background(glassFill).border(1.dp, glassBorder, RoundedCornerShape(12.dp)).padding(8.dp, 5.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(icon, null, tint = goldInk, modifier = Modifier.size(16.dp))
        Spacer(Modifier.width(5.dp))
        Text(text, color = ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
    }
}

/** One public-address row in the connection card: label + the IP (mono, tap to copy). */
@Composable
private fun PublicAddrRow(label: String, addr: String, accent: Color) {
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).clickable {
            clipboard.setText(AnnotatedString(addr))
            android.widget.Toast.makeText(ctx, "$label copied", android.widget.Toast.LENGTH_SHORT).show()
        }.padding(vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Public, null, tint = accent, modifier = Modifier.size(14.dp))
        Spacer(Modifier.width(6.dp))
        Text("$label  ", color = muted, fontSize = 11.sp)
        Text(addr, color = ink, fontSize = 11.sp, fontFamily = mono, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        Spacer(Modifier.width(6.dp))
        Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(12.dp))
    }
}

@Composable
private fun ConnStep(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, body: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        Box(Modifier.size(30.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
            Icon(icon, null, tint = Navy, modifier = Modifier.size(17.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text(body, color = muted, fontSize = 13.sp)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EditProfileSheet(current: Profile, onClose: () -> Unit, onSaved: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var nickname by remember { mutableStateOf(current.nickname) }
    var bio by remember { mutableStateOf(current.bio) }
    var avatarCid by remember { mutableStateOf(current.avatar) }
    var avatar by remember { mutableStateOf<Bitmap?>(null) }
    var avatarBytes by remember { mutableStateOf<ByteArray?>(null) }
    var busy by remember { mutableStateOf(false) }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val b = withContext(Dispatchers.IO) { runCatching { scaleWebp(ctx.contentResolver.openInputStream(uri)!!.use { it.readBytes() }, 512, 82) }.getOrNull() }
            if (b != null) { avatarBytes = b; avatar = BitmapFactory.decodeByteArray(b, 0, b.size) }
        }
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
            Text("Edit profile", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(16.dp))
            Box(Modifier.size(88.dp).clip(RoundedCornerShape(44.dp)).background(Brush.linearGradient(listOf(Gold, Gold2)))
                .clickable { pick.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }, Alignment.Center) {
                when {
                    avatar != null -> Image(avatar!!.asImageBitmap(), null, Modifier.fillMaxSize().clip(RoundedCornerShape(44.dp)), contentScale = ContentScale.Crop)
                    avatarCid.isNotBlank() -> AsyncImage(HeyApi.mediaUri(avatarCid), null, Modifier.fillMaxSize().clip(RoundedCornerShape(44.dp)), contentScale = ContentScale.Crop)
                    else -> Icon(Icons.Filled.AddAPhoto, null, tint = Navy, modifier = Modifier.size(32.dp))
                }
            }
            Spacer(Modifier.height(16.dp))
            OutlinedTextField(value = nickname, onValueChange = { nickname = it }, placeholder = { Text("Nickname", color = muted) },
                singleLine = true, modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(), textStyle = androidx.compose.ui.text.TextStyle(color = ink))
            Spacer(Modifier.height(10.dp))
            OutlinedTextField(value = bio, onValueChange = { bio = it }, placeholder = { Text("Bio", color = muted) },
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(), textStyle = androidx.compose.ui.text.TextStyle(color = ink))
            Spacer(Modifier.height(18.dp))
            Button(
                onClick = {
                    busy = true
                    scope.launch {
                        withContext(Dispatchers.IO) {
                            runCatching {
                                avatarBytes?.let { val t = HeyApi.uploadMedia(it, "image/webp", "avatar.webp"); avatarCid = t.optString("cid") }
                                HeyApi.setProfile(nickname.trim().ifBlank { "Hey user" }, bio.trim(), avatarCid)
                            }
                        }
                        busy = false; onSaved()
                    }
                },
                enabled = !busy, modifier = Modifier.fillMaxWidth().height(50.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
            ) { if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp) else Text("Save", fontWeight = FontWeight.Bold) }
            Spacer(Modifier.height(24.dp))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MyQrSheet(did: String, onClose: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    var link by remember { mutableStateOf("") }
    var qr by remember { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            runCatching { link = HeyApi.friendLink(); qr = qrBitmap(QrLink.toQr(link)) }
        }
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
            Text("Add me on Hey", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text("Best: tap Share and send the link. Or scan the QR up close in good light.", color = muted, fontSize = 12.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Spacer(Modifier.height(14.dp))
            // As large as the sheet allows → biggest QR cells → most scannable.
            Box(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(16.dp)).background(Color.White).padding(10.dp), Alignment.Center) {
                when {
                    qr != null -> Image(qr!!.asImageBitmap(), "QR", Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                    link.isBlank() -> CircularProgressIndicator(color = Navy)
                    else -> Text("Use Share / Copy below", color = Navy)
                }
            }
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { shareText(ctx, link) }, enabled = link.isNotBlank(),
                    colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    Icon(Icons.Filled.Share, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Share link", fontWeight = FontWeight.Bold)
                }
                OutlinedButton(onClick = { clipboard.setText(AnnotatedString(link)) }, enabled = link.isNotBlank()) {
                    Icon(Icons.Filled.ContentCopy, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Copy", color = ink)
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AddFriendSheet(onClose: () -> Unit, onFollowed: () -> Unit) {
    val scope = rememberCoroutineScope()
    var input by remember { mutableStateOf("") }
    var status by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    val scanner = rememberLauncherForActivityResult(ScanContract()) { result ->
        result.contents?.let { input = QrLink.fromScan(it) }
    }
    fun doFollow() {
        val v = input.trim()
        if (v.isEmpty()) { status = "Paste a Hey friend link or scan a QR"; return }
        // A bare DID carries no PQ keys, so it can't open a private channel —
        // require the friend link/QR (which bundles the encryption keys + ticket).
        if (v.startsWith("did:") && !v.contains("hey:follow")) {
            status = "That's a DID — it can't start a private channel. Ask them for their Hey friend link or QR."
            return
        }
        busy = true; status = "Connecting…"
        scope.launch {
            val res = withContext(Dispatchers.IO) { runCatching { HeyApi.follow(v) }.getOrNull() }
            busy = false
            if (res != null && !res.has("error")) onFollowed()
            else status = "Failed: ${res?.optString("error") ?: "invalid"}"
        }
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("Follow someone", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text("Paste their Hey friend link, or scan their QR.", color = muted, fontSize = 13.sp)
            Spacer(Modifier.height(14.dp))
            // singleLine so a long pasted/scanned link stays one row and never
            // pushes the buttons off the sheet.
            OutlinedTextField(
                value = input, onValueChange = { input = it }, singleLine = true, maxLines = 1,
                placeholder = { Text("hey:follow:…", color = muted, fontSize = 13.sp) },
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink, fontSize = 13.sp),
            )
            if (input.length > 24) {
                Spacer(Modifier.height(4.dp))
                Text("✓ Link ready (${input.length} chars)", color = good, fontSize = 11.sp)
            }
            Spacer(Modifier.height(12.dp))
            Row {
                OutlinedButton(onClick = {
                    scanner.launch(ScanOptions().setDesiredBarcodeFormats(ScanOptions.QR_CODE).setOrientationLocked(false).setBeepEnabled(false).setPrompt("Scan a Hey QR").setCaptureActivity(PortraitCaptureActivity::class.java))
                }) { Icon(Icons.Filled.QrCodeScanner, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Scan QR", color = ink) }
                Spacer(Modifier.width(12.dp))
                Button(onClick = { doFollow() }, enabled = !busy,
                    colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    Text("Follow", fontWeight = FontWeight.Bold)
                }
            }
            if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = muted, fontSize = 13.sp) }
            Spacer(Modifier.height(24.dp))
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

@Composable
private fun glassFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = ink, unfocusedTextColor = ink,
    focusedBorderColor = Gold, unfocusedBorderColor = glassBorder,
    cursorColor = Gold,
    focusedContainerColor = glassFill,
    unfocusedContainerColor = glassFill,
)

/**
 * The friend link (`hey:follow:<base64url>`) carries the ML-KEM-768 key, so it's
 * ~3 KB — right at QR's byte-mode ceiling (2953 B), which made the QR fail to
 * render. For the QR ONLY we re-encode the RAW payload in base32/uppercase and
 * tag it `HEYF`, so every character is in QR's alphanumeric set → zxing uses the
 * denser alphanumeric mode (~4296-char capacity, ~22% headroom here). The pasted
 * link stays the nice lowercase base64url form; only the scanned QR uses this.
 */
private object QrLink {
    private const val A = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
    private const val TAG = "HEYF"
    private const val B64 = android.util.Base64.URL_SAFE or android.util.Base64.NO_PADDING or android.util.Base64.NO_WRAP

    /** friend link → compact alphanumeric QR payload (falls back to the link). */
    fun toQr(link: String): String {
        val b64 = link.substringAfter("hey:follow:", "")
        if (b64.isEmpty()) return link
        val raw = runCatching { android.util.Base64.decode(b64, B64) }.getOrNull() ?: return link
        return TAG + b32enc(raw)
    }

    /** scanned text → original friend link if it's our tagged QR, else unchanged. */
    fun fromScan(s: String): String {
        val t = s.trim()
        if (!t.startsWith(TAG)) return t
        val raw = runCatching { b32dec(t.removePrefix(TAG)) }.getOrNull() ?: return t
        return "hey:follow:" + android.util.Base64.encodeToString(raw, B64)
    }

    private fun b32enc(data: ByteArray): String {
        val sb = StringBuilder(); var buf = 0; var bits = 0
        for (b in data) {
            buf = (buf shl 8) or (b.toInt() and 0xff); bits += 8
            while (bits >= 5) { bits -= 5; sb.append(A[(buf ushr bits) and 0x1f]) }
        }
        if (bits > 0) sb.append(A[(buf shl (5 - bits)) and 0x1f])
        return sb.toString()
    }
    private fun b32dec(s: String): ByteArray {
        val out = java.io.ByteArrayOutputStream(); var buf = 0; var bits = 0
        for (c in s) {
            val v = A.indexOf(c); if (v < 0) continue
            buf = (buf shl 5) or v; bits += 5
            if (bits >= 8) { bits -= 8; out.write((buf ushr bits) and 0xff) }
        }
        return out.toByteArray()
    }
}

// EC level L + byte charset = max capacity, so ~1KB invite links still encode
// (default EC throws on them). Returns null on failure so the UI can fall back
// to copy-only instead of a stuck spinner.
private fun qrBitmap(text: String, size: Int = 880): Bitmap? {
    if (text.isBlank()) return null
    // Prefer L: for a big payload (the friend link carries a ~1.2 KB PQ key) L
    // packs into the FEWEST modules → the largest, most camera-readable cells.
    // (M would push to a denser version 40, which phones struggle to scan.)
    for (ec in listOf(
        com.google.zxing.qrcode.decoder.ErrorCorrectionLevel.L,
        com.google.zxing.qrcode.decoder.ErrorCorrectionLevel.M,
    )) {
        val bmp = runCatching {
            val hints = mapOf(
                com.google.zxing.EncodeHintType.ERROR_CORRECTION to ec,
                com.google.zxing.EncodeHintType.CHARACTER_SET to "ISO-8859-1",
                com.google.zxing.EncodeHintType.MARGIN to 1,
            )
            val matrix = com.google.zxing.qrcode.QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size, hints)
            val b = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
            for (x in 0 until size) for (y in 0 until size) {
                b.setPixel(x, y, if (matrix[x, y]) android.graphics.Color.BLACK else android.graphics.Color.WHITE)
            }
            b
        }.getOrNull()
        if (bmp != null) return bmp
    }
    return null
}

/**
 * Decode + downscale to <=2048px and re-encode to WebP (lossy q80) — hardware
 * accelerated, tiny (~50-150 KB for a phone photo), near-AVIF quality. WebP is
 * natively supported for encode (here) AND decode (Coil); AVIF on Android can
 * only be decoded (14+), not encoded, without a heavy CPU-bound native encoder.
 */
private fun scaleWebp(raw: ByteArray, maxDim: Int = 2048, quality: Int = 80): ByteArray {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(raw, 0, raw.size, bounds)
    var sample = 1
    val longest = maxOf(bounds.outWidth, bounds.outHeight)
    while (longest / sample > maxDim) sample *= 2
    var bmp = BitmapFactory.decodeByteArray(raw, 0, raw.size, BitmapFactory.Options().apply { inSampleSize = sample })
        ?: return raw
    // Bake in the EXIF orientation. Cameras store portrait shots as landscape pixels + an orientation
    // tag; re-encoding to WebP drops that tag, so without this a portrait photo arrives sideways.
    bmp = applyExifOrientation(raw, bmp)
    val out = ByteArrayOutputStream()
    val fmt = if (android.os.Build.VERSION.SDK_INT >= 30) {
        Bitmap.CompressFormat.WEBP_LOSSY
    } else {
        @Suppress("DEPRECATION") Bitmap.CompressFormat.WEBP
    }
    bmp.compress(fmt, quality, out)
    return out.toByteArray()
}

/** Rotate/flip a decoded bitmap per the source bytes' EXIF orientation, so re-encoding preserves
 *  the way the photo was actually shot (the orientation tag is lost on re-encode). */
private fun applyExifOrientation(raw: ByteArray, bmp: Bitmap): Bitmap {
    val orientation = runCatching {
        android.media.ExifInterface(java.io.ByteArrayInputStream(raw))
            .getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, android.media.ExifInterface.ORIENTATION_NORMAL)
    }.getOrDefault(android.media.ExifInterface.ORIENTATION_NORMAL)
    val m = android.graphics.Matrix()
    when (orientation) {
        android.media.ExifInterface.ORIENTATION_ROTATE_90 -> m.postRotate(90f)
        android.media.ExifInterface.ORIENTATION_ROTATE_180 -> m.postRotate(180f)
        android.media.ExifInterface.ORIENTATION_ROTATE_270 -> m.postRotate(270f)
        android.media.ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> m.postScale(-1f, 1f)
        android.media.ExifInterface.ORIENTATION_FLIP_VERTICAL -> m.postScale(1f, -1f)
        android.media.ExifInterface.ORIENTATION_TRANSPOSE -> { m.postRotate(90f); m.postScale(-1f, 1f) }
        android.media.ExifInterface.ORIENTATION_TRANSVERSE -> { m.postRotate(270f); m.postScale(-1f, 1f) }
        else -> return bmp // NORMAL / UNDEFINED → already upright
    }
    return runCatching { Bitmap.createBitmap(bmp, 0, 0, bmp.width, bmp.height, m, true) }.getOrDefault(bmp)
}

/** Lower ceiling when the contact is reachable only over the shared RELAY: a big
 *  file would flood it, and the relay forwards bytes for everyone. DIRECT P2P pays
 *  no relay cost, so it uses the higher direct ceiling. (Direct is still RAM-bounded
 *  until the streaming rework lifts it toward "unlimited".) */
private const val RELAY_ATTACH_BYTES = 16L * 1024 * 1024

/** Reported size of a content Uri in bytes, or -1 if unknown (then we don't block). */
private fun uriSize(ctx: android.content.Context, uri: Uri): Long = runCatching {
    ctx.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.SIZE), null, null, null)?.use { c ->
        if (c.moveToFirst()) {
            val i = c.getColumnIndex(android.provider.OpenableColumns.SIZE)
            if (i >= 0 && !c.isNull(i)) c.getLong(i) else -1L
        } else -1L
    } ?: -1L
}.getOrDefault(-1L)

/** Effectively-unlimited ceiling for the STREAMED (direct) path — a 16 GB sanity
 *  bound mirroring hey-core's MAX_STREAMED_ATTACHMENT_BYTES. Real limit = disk. */
private const val MAX_STREAMED_BYTES = 16L * 1024 * 1024 * 1024

/** Copy a picked content Uri to a temp file via a CHUNKED stream (never the whole
 *  file in RAM) so the streamed sender can read it by path. (file, mime, name). */
private fun copyUriToTemp(ctx: android.content.Context, uri: Uri): Triple<java.io.File, String, String>? = runCatching {
    val cr = ctx.contentResolver
    val mime = cr.getType(uri) ?: "application/octet-stream"
    var name = "file"
    cr.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0)?.let { name = it }
    }
    val dir = java.io.File(ctx.cacheDir, "outbox").apply { mkdirs() }
    val safe = name.replace(Regex("[^A-Za-z0-9._-]"), "_")
    val tmp = java.io.File(dir, "send-${android.os.SystemClock.elapsedRealtime()}-$safe")
    val copied = cr.openInputStream(uri)?.use { input -> tmp.outputStream().use { out -> input.copyTo(out, 256 * 1024) } }
    if (copied == null) { tmp.delete(); return@runCatching null }
    Triple(tmp, mime, name)
}.getOrNull()

/** Read a picked content Uri into (bytes, mime, display-name). */
private fun readUri(ctx: android.content.Context, uri: Uri): Triple<ByteArray, String, String>? {
    val cr = ctx.contentResolver
    val mime = cr.getType(uri) ?: "application/octet-stream"
    var name = "file"
    runCatching {
        cr.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0)?.let { name = it }
        }
    }
    val bytes = runCatching { cr.openInputStream(uri)?.use { it.readBytes() } }.getOrNull() ?: return null
    return Triple(bytes, mime, name)
}

/** Ref-counted FLAG_KEEP_SCREEN_ON so the screen doesn't auto-lock mid-transfer —
 *  locking the phone stops a live P2P transfer, so while a file is moving we keep
 *  the screen awake like a video player (no battery-eating wake lock). */
private object KeepAwake {
    private var count = 0
    fun on(ctx: android.content.Context) {
        val act = ctx as? android.app.Activity ?: return
        synchronized(this) {
            count++
            if (count == 1) act.runOnUiThread { act.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        }
    }
    fun off(ctx: android.content.Context) {
        val act = ctx as? android.app.Activity ?: return
        synchronized(this) {
            if (count > 0) count--
            if (count == 0) act.runOnUiThread { act.window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
        }
    }
}

/** Stable per-attachment cache file (keyed by the attachment id) so a streamed
 *  download RESUMES across re-taps instead of restarting from 0%. */
private fun attCacheFile(ctx: android.content.Context, att: Attachment): java.io.File {
    val dir = java.io.File(ctx.cacheDir, "attachments").apply { mkdirs() }
    val key = runCatching { org.json.JSONObject(att.raw).optString("id") }.getOrDefault("")
        .ifBlank { att.raw.hashCode().toString() }
    val safe = att.name.ifBlank { "file" }.replace(Regex("[^A-Za-z0-9._-]"), "_")
    return java.io.File(dir, "$key-$safe")
}

/** Fetch a streamed attachment to `dest`, AUTO-RETRYING — each retry RESUMES from
 *  where the last attempt stopped (hey-core skips already-downloaded chunks), so a
 *  stalled transfer climbs to 100% instead of restarting. Keeps the screen awake. */
private suspend fun fetchStreamedResilient(ctx: android.content.Context, att: Attachment, dest: java.io.File): java.io.File? {
    KeepAwake.on(ctx)
    try {
        repeat(4) { attempt ->
            val r = HeyApi.fetchAttachmentToPath(att, dest)
            if (r.isSuccess) return r.getOrNull()
            if (attempt < 3) kotlinx.coroutines.delay(1500)
        }
        return null
    } finally {
        KeepAwake.off(ctx)
    }
}

/** Fetch + decrypt an attachment to a cache file and open it in an external app.
 *  Surfaces a real reason on failure instead of silently doing nothing — a large
 *  attachment is fetched DIRECT P2P from the holder, so it can fail when the
 *  sender is offline/backgrounded or the link hasn't formed. */
private suspend fun openAttachment(ctx: android.content.Context, att: Attachment) {
    val file = withContext(Dispatchers.IO) {
        runCatching {
            if (att.isStreamed) {
                // Torrent-style: download + decrypt to disk (O(chunk) RAM), resumable + screen-awake.
                fetchStreamedResilient(ctx, att, attCacheFile(ctx, att))
            } else {
                val f = attCacheFile(ctx, att)
                val bytes = HeyApi.fetchAttachment(att)
                if (bytes.isEmpty()) null else f.apply { writeBytes(bytes) }
            }
        }.getOrNull()
    }
    if (file == null) {
        android.widget.Toast.makeText(
            ctx,
            "Couldn't fetch “${att.name.ifBlank { "file" }}” — the sender may be offline. Try again when you're both online.",
            android.widget.Toast.LENGTH_LONG
        ).show()
        return
    }
    val uri = runCatching { androidx.core.content.FileProvider.getUriForFile(ctx, ctx.packageName + ".files", file) }.getOrNull() ?: return
    runCatching {
        val i = android.content.Intent(android.content.Intent.ACTION_VIEW)
            .setDataAndType(uri, att.mime)
            .addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ctx.startActivity(android.content.Intent.createChooser(i, "Open ${att.name}"))
    }
}

/** Fetch + decrypt an attachment and SAVE it to the device via MediaStore —
 *  video → Movies/Hey, anything else → Downloads/Hey. Returns true on success. */
private suspend fun saveAttachment(ctx: android.content.Context, att: Attachment): Boolean = withContext(Dispatchers.IO) {
    runCatching {
        // 1. Get the plaintext onto a local file FIRST (streamed → resumable + screen-awake;
        //    small → bytes). Doing the (possibly long) fetch before touching MediaStore avoids
        //    leaving a half-written gallery entry open during the download.
        val src: java.io.File = if (att.isStreamed) {
            fetchStreamedResilient(ctx, att, attCacheFile(ctx, att)) ?: return@runCatching false
        } else {
            val bytes = HeyApi.fetchAttachment(att)
            if (bytes.isEmpty()) return@runCatching false
            attCacheFile(ctx, att).apply { writeBytes(bytes) }
        }
        // 2. Stream the local file into MediaStore (never the whole file in RAM).
        val isVideo = att.isVideo || att.mime.startsWith("video/")
        val isImage = att.isImage || att.mime.startsWith("image/")
        val safeName = att.name.ifBlank { if (isVideo) "hey-video.mp4" else "hey-file" }
        val values = android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, safeName)
            put(android.provider.MediaStore.MediaColumns.MIME_TYPE, att.mime.ifBlank { "application/octet-stream" })
            if (Build.VERSION.SDK_INT >= 29) put(
                android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                (if (isVideo) android.os.Environment.DIRECTORY_MOVIES else android.os.Environment.DIRECTORY_DOWNLOADS) + "/Hey"
            )
        }
        // Pick a collection that exists on this API level (Downloads is API 29+;
        // the SDK_INT branch keeps lintVitalRelease happy at minSdk 26).
        val collection = when {
            isVideo -> android.provider.MediaStore.Video.Media.EXTERNAL_CONTENT_URI
            isImage -> android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI
            Build.VERSION.SDK_INT >= 29 -> android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI
            else -> android.provider.MediaStore.Files.getContentUri("external")
        }
        val resolver = ctx.contentResolver
        val uri = resolver.insert(collection, values) ?: return@runCatching false
        val ok = runCatching {
            resolver.openOutputStream(uri)?.use { o -> src.inputStream().use { it.copyTo(o, 256 * 1024) }; true } ?: false
        }.getOrDefault(false)
        if (!ok) runCatching { resolver.delete(uri, null, null) } // don't leave an empty file
        ok
    }.getOrDefault(false)
}

/** Turn the StrongBox/TEE vault ON: CryptoObject-bound seal of the live seed →
 *  CryptoObject-bound round-trip verify → only THEN delete the plaintext. Never
 *  destroys the seed on failure (no verify, no delete). Atomic ordering UNCHANGED:
 *  seal → verify → persistCarrierIdentity()==0 → setOn(true) → deleteIdentity LAST.
 *
 *  H2.1: seal + verify are each gated by a fresh, Hey-initiated BiometricPrompt
 *  CryptoObject (no 30s window). H2.2: the CALLER must have already proven the
 *  recovery phrase is recorded before invoking this — there is no seal-without-phrase
 *  path on the default-on/migration routes.
 *
 *  The seal value is the BARE recovery phrase (what unlockedSeed holds after a
 *  create/restore, or what recoveryPhrase() returns): hey_unlock does from_mnemonic,
 *  so this is correct — the runtime re-derives every key from the words on unseal. */
private fun enableVault(
    activity: androidx.fragment.app.FragmentActivity, ctx: android.content.Context,
    scope: kotlinx.coroutines.CoroutineScope, onResult: (Boolean) -> Unit,
) {
    // Seed from the in-memory runtime (identity.json is encrypted at rest). H5: the
    // bare hey_recovery_phrase JNI refuses while the hardware spend/reveal binding is
    // active, so when enrolled we obtain the seed through the signature-verified reveal.
    scope.launch {
        val plaintext = HeyApi.unlockedSeed
            ?: (if (SpendAuth.isEnrolled(ctx)) SpendAuth.revealSeed(activity) else HeyApi.recoveryPhrase())
        if (plaintext.isNullOrBlank()) { onResult(false); return@launch }
        // 1) SEAL under a fresh CryptoObject-bound biometric (this prompt IS the auth —
        //    the encrypt op is gated by it, so no separate AppLock prompt is needed).
        val sealed = kotlinx.coroutines.suspendCancellableCoroutine<Boolean> { cont ->
            IdentityVault.sealAuthed(activity, plaintext) { ok -> cont.resumeWith(Result.success(ok)) }
        }
        if (!sealed) { onResult(false); return@launch }
        // 2) ROUND-TRIP VERIFY under a second CryptoObject-bound biometric: prove the
        //    seal actually decrypts to the same seed BEFORE we ever delete the plaintext.
        val verified = kotlinx.coroutines.suspendCancellableCoroutine<String?> { cont ->
            IdentityVault.unsealAuthed(activity) { s, _ -> cont.resumeWith(Result.success(s)) }
        }
        if (verified != plaintext) {
            withContext(Dispatchers.IO) { IdentityVault.clear(ctx) } // bad/unverifiable seal: abort, keep identity.json
            onResult(false); return@launch
        }
        // 3) Persist the headless carrier blob BEFORE removing the on-disk seed, then
        //    flip on + delete LAST. A crash anywhere here leaves identity.json intact.
        val done = withContext(Dispatchers.IO) {
            if (HeyApi.persistCarrierIdentity() != 0) { IdentityVault.clear(ctx); return@withContext false }
            IdentityVault.setOn(ctx, true)
            HeyApi.unlockedSeed = plaintext
            HeyApi.deleteIdentity(ctx) // plaintext gone — seal proven recoverable + blob in place
            true
        }
        onResult(done)
    }
}

/** Turn the vault OFF: CryptoObject-bound unseal → write the plaintext seed back
 *  → wipe the keystore key + sealed blob.
 *
 *  H2.8: this is NOT reachable from the Settings switch (the OFF direction is disabled
 *  once on), so turning the vault off — re-creating the pentest-vulnerable no-auth-seed
 *  posture — requires a deliberate code path. If ever re-exposed in the UI, gate it
 *  behind an explicit "your seed will be stored without a fingerprint" warning + the
 *  same recovery-phrase-confirm as enableVault. The unseal here is itself bound to a
 *  fresh BiometricPrompt CryptoObject (H2.1) — no 30s window. */
private fun disableVault(
    activity: androidx.fragment.app.FragmentActivity, ctx: android.content.Context,
    scope: kotlinx.coroutines.CoroutineScope, onResult: (Boolean) -> Unit,
) {
    IdentityVault.unsealAuthed(activity) { unsealed, _ ->
        val seed = unsealed ?: HeyApi.unlockedSeed
        if (seed.isNullOrBlank()) { onResult(false); return@unsealAuthed }
        scope.launch {
            val done = withContext(Dispatchers.IO) {
                HeyApi.unlockedSeed = seed
                // Retain the seed for next launch as a well-formed IdentityBlob SEALED
                // under the storage DEK (never plaintext, never a bare mnemonic). If the
                // runtime can't persist it, ABORT atomically — keep the seal + vault ON
                // rather than write an unparseable file that mints a fresh identity on
                // the next cold start (= silent account loss). Mirrors enableVault.
                if (HeyApi.persistIdentity() != 0) return@withContext false
                IdentityVault.clear(ctx)
                true
            }
            onResult(done)
        }
    }
}

/** Recovery — the BIP39 12-word phrase (the one root: recovers your Hey account
 *  AND the matching Elastos DID + wallet in official Essentials). In a running
 *  session it's parsed from the unsealed identity in memory; with the vault off,
 *  from the plaintext file. Falls back to the raw blob for legacy identities. */
private fun identityBackup(ctx: android.content.Context): String? {
    // In-memory seed first; once the runtime is up ask it (identity.json is
    // encrypted at rest). Legacy plaintext file only as a last resort.
    val blob = HeyApi.unlockedSeed ?: HeyApi.recoveryPhrase() ?: HeyApi.readIdentity(ctx) ?: return null
    return runCatching { org.json.JSONObject(blob).optString("mnemonic").ifBlank { blob } }.getOrDefault(blob)
}

/** Share a Hey link via any app (the reliable way to send the big friend link
 *  — its embedded post-quantum key makes the QR very dense to scan). */
private fun shareText(ctx: android.content.Context, text: String) {
    runCatching {
        val i = android.content.Intent(android.content.Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(android.content.Intent.EXTRA_TEXT, text)
        ctx.startActivity(android.content.Intent.createChooser(i, "Share your Hey link"))
    }
}

private fun humanSize(bytes: Long): String = when {
    bytes >= 1_000_000 -> "%.1f MB".format(bytes / 1_000_000.0)
    bytes >= 1_000 -> "%.0f KB".format(bytes / 1_000.0)
    else -> "$bytes B"
}

// ── wallet (ESC — one mnemonic, recoverable in official Elastos Essentials) ──

private fun shortAddr(a: String) = if (a.length > 14) "${a.take(8)}…${a.takeLast(6)}" else a
private val mono = androidx.compose.ui.text.font.FontFamily.Monospace

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WalletScreen(topPad: Dp = 12.dp) {
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val prefs = remember { ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE) }
    // The Essentials-compatibility note is read-once: dismissable + remembered.
    var essentialsDismissed by remember { mutableStateOf(prefs.getBoolean("essentials_note_dismissed", false)) }
    // First time on the Wallet tab: elegant DID + wallet generation, then reveal.
    var setupDone by remember { mutableStateOf(prefs.getBoolean("elastos_setup", false)) }
    if (!setupDone) {
        DidGenerationScreen(onDone = { prefs.edit().putBoolean("elastos_setup", true).apply(); setupDone = true })
        return
    }
    // Chains come from the Rust registry (elastos://<chain>/) — adding one there
    // makes it appear here automatically. EVM chains share one 0x address; the
    // Elastos mainchain (E…) is receive-only for now.
    val chains = remember {
        val evm = HeyApi.walletChains()
        // Order: Elastos Mainchain first, then the EVM chains (ESC, Ethereum, Base, EID), BEAM last.
        // USD stablecoins (USDT/USDC) appear inside each EVM chain's token sheet.
        val evmOrder = listOf("esc", "ethereum", "base", "eid")
        val byKey = evm.associateBy { it.key }
        val orderedEvm = (evmOrder.mapNotNull { byKey[it] } + evm.filter { it.key !in evmOrder })
            .map { UiChain(it.key, it.name, "${it.symbol} · EVM", true, it.symbol) }
        val out = mutableListOf(UiChain("ela", "Elastos Mainchain", "ELA · mainchain", false, "ELA"))
        if (orderedEvm.isEmpty()) out.add(UiChain("esc", "Elastos Smart Chain", "ELA · EVM", true, "ELA"))
        else out.addAll(orderedEvm)
        if (BeamApi.available) out.add(UiChain("beam", "BEAM", "Mimblewimble · private", false, "BEAM"))
        out
    }
    var evmAddr by remember { mutableStateOf<String?>(null) }
    var elaAddr by remember { mutableStateOf<String?>(null) }
    var beamAddr by remember { mutableStateOf<String?>(null) }
    var did by remember { mutableStateOf<String?>(null) }
    var bal by remember { mutableStateOf<Map<String, String?>>(emptyMap()) }
    var loading by remember { mutableStateOf(true) }
    var showSend by remember { mutableStateOf(false) }
    var showReceive by remember { mutableStateOf<UiChain?>(null) }
    var showTokens by remember { mutableStateOf<UiChain?>(null) }
    var showNfts by remember { mutableStateOf<UiChain?>(null) }
    var showSettings by remember { mutableStateOf(false) }
    var showElaSend by remember { mutableStateOf(false) }
    var showBeamAssets by remember { mutableStateOf(false) }
    var showBeamSend by remember { mutableStateOf(false) }
    var refreshKey by remember { mutableStateOf(0) }
    val pager = rememberPagerState(pageCount = { chains.size })
    val active = chains[pager.currentPage.coerceIn(0, chains.size - 1)]
    val activeAddr = when { active.evm -> evmAddr; active.key == "beam" -> beamAddr; else -> elaAddr }

    LaunchedEffect(refreshKey) {
        loading = true
        withContext(Dispatchers.IO) {
            runCatching {
                did = HeyApi.elastosDid(ctx)
                evmAddr = HeyApi.walletAddress(ctx)
                elaAddr = HeyApi.elaAddress(ctx)
                beamAddr = HeyApi.beamAddress(ctx)   // public_offline token (local mint via libbeam.so)
                val m = HashMap<String, String?>()
                chains.filter { it.evm }.forEach { m[it.key] = runCatching { HeyApi.walletBalance(ctx, it.key)?.balance }.getOrNull() }
                m["ela"] = runCatching { HeyApi.elaBalance(ctx) }.getOrNull() // mainchain UTXO balance
                if (BeamApi.available) m["beam"] = runCatching { HeyApi.beamBalance(ctx)?.beam }.getOrNull() // last-synced
                bal = m
                // Publish receive addresses so followers can tip by identity ("just works").
                // Re-publish once when BEAM lands so existing users get their "beam" donation address out.
                val needPublish = !prefs.getBoolean("tips_published", false) ||
                    (BeamApi.available && !prefs.getBoolean("beam_published", false))
                if (needPublish && HeyApi.publishTipAddresses(ctx)) {
                    prefs.edit().putBoolean("tips_published", true).putBoolean("beam_published", true).apply()
                }
            }
        }
        loading = false
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp, topPad, 20.dp, 110.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Wallet", color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                Text("Elastos identity + chains", color = muted, fontSize = 12.sp)
            }
            IconButton(onClick = { showSettings = true }) { Icon(Icons.Filled.Settings, "Wallet settings", tint = muted) }
        }
        Spacer(Modifier.height(12.dp))

        // Your Elastos DID — the wallet's umbrella identity (EID).
        did?.let { d ->
            Column(Modifier.fillMaxWidth().glass(18.dp).padding(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Badge, null, tint = goldInk, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Your Elastos DID", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                    Spacer(Modifier.weight(1f))
                    Text("EID", color = muted, fontSize = 11.sp)
                }
                Spacer(Modifier.height(8.dp))
                Row(
                    Modifier.clip(RoundedCornerShape(10.dp)).background(Color.Black.copy(alpha = 0.10f))
                        .clickable {
                            clipboard.setText(AnnotatedString(d))
                            android.widget.Toast.makeText(ctx, "DID copied", android.widget.Toast.LENGTH_SHORT).show()
                        }.padding(10.dp, 8.dp).fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(d.removePrefix("did:elastos:").let { "did:elastos:${it.take(8)}…${it.takeLast(6)}" },
                        color = ink, fontSize = 12.sp, fontFamily = mono, modifier = Modifier.weight(1f))
                    Icon(Icons.Filled.ContentCopy, "Copy DID", tint = muted, modifier = Modifier.size(14.dp))
                }
            }
            Spacer(Modifier.height(14.dp))
        }

        // Stacked chain cards — swipe ESC ↔ Mainchain.
        HorizontalPager(
            state = pager, pageSpacing = 12.dp,
            contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 2.dp),
            modifier = Modifier.fillMaxWidth()
        ) { page ->
            val c = chains[page]
            ChainCard(
                chain = c,
                address = when { c.evm -> evmAddr; c.key == "beam" -> beamAddr; else -> elaAddr },
                balance = bal[c.key],
                loading = loading,
                onClick = { if (c.evm) showTokens = c else if (c.key == "beam") showBeamAssets = true }, // tap → tokens / BEAM assets
                onCopy = { a ->
                    clipboard.setText(AnnotatedString(a))
                    android.widget.Toast.makeText(ctx, "Address copied", android.widget.Toast.LENGTH_SHORT).show()
                },
            )
        }
        Spacer(Modifier.height(12.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
            chains.forEachIndexed { i, _ ->
                Box(
                    Modifier.padding(horizontal = 4.dp).size(if (pager.currentPage == i) 9.dp else 7.dp)
                        .clip(CircleShape).background(if (pager.currentPage == i) goldInk else muted.copy(alpha = 0.4f))
                )
            }
        }
        Spacer(Modifier.height(16.dp))

        // Collectibles — EVM-only (ESC/EID/Ethereum); hidden on ELA + BEAM.
        if (active.evm) {
            CollectiblesCard(chain = active, addr = evmAddr, onClick = { showNfts = active })
            Spacer(Modifier.height(16.dp))
        }

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(
                onClick = {
                    when {
                        active.evm -> showSend = true
                        active.key == "beam" -> showBeamSend = true
                        else -> showElaSend = true
                    }
                }, enabled = activeAddr != null, modifier = Modifier.weight(1f),
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
            ) { Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Send", fontWeight = FontWeight.Bold) }
            OutlinedButton(onClick = { showReceive = active }, enabled = activeAddr != null, modifier = Modifier.weight(1f)) {
                Icon(Icons.Filled.QrCode2, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Receive", color = ink)
            }
        }
        Spacer(Modifier.height(6.dp))
        TextButton(onClick = { refreshKey++ }) {
            Icon(Icons.Filled.Refresh, null, Modifier.size(16.dp), tint = muted); Spacer(Modifier.width(4.dp)); Text("Refresh", color = muted, fontSize = 13.sp)
        }
        Spacer(Modifier.height(10.dp))

        if (!essentialsDismissed) {
            Row(Modifier.fillMaxWidth().glass(16.dp).padding(14.dp), verticalAlignment = Alignment.Top) {
                Icon(Icons.Filled.Shield, null, tint = goldInk, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text("Same wallets as Elastos Essentials", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                    Spacer(Modifier.height(3.dp))
                    Text(
                        "Every address here is derived from your recovery phrase. Import that phrase into official Elastos Essentials and you'll see the same DID + wallets. Your keys never leave the phone.",
                        color = muted, fontSize = 12.sp
                    )
                }
                Spacer(Modifier.width(8.dp))
                Icon(
                    Icons.Filled.Close, "Dismiss", tint = muted,
                    modifier = Modifier.size(18.dp).clickable {
                        essentialsDismissed = true
                        prefs.edit().putBoolean("essentials_note_dismissed", true).apply()
                    }
                )
            }
            Spacer(Modifier.height(10.dp))
        }

        // Transaction history (toggle in the gear) — your sends + tips, recorded locally.
        if (HeyApi.showTxHistory(ctx)) {
            val history = remember(refreshKey) { HeyApi.txHistory(ctx) }
            Spacer(Modifier.height(18.dp))
            Text("Recent activity", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            Spacer(Modifier.height(8.dp))
            if (history.isEmpty()) {
                Text("Your sends and tips will show here.", color = muted, fontSize = 12.sp)
            } else {
                history.take(25).forEach { t -> TxRow(t) }
            }
        }
    }

    showReceive?.let { c ->
        val a = when { c.evm -> evmAddr; c.key == "beam" -> beamAddr; else -> elaAddr }
        a?.let { ReceiveSheet(it, c.title, c.sub, c.symbol) { showReceive = null } }
    }
    if (showSend) evmAddr?.let { SendSheet(chain = active.key, symbol = active.symbol, network = active.title, onClose = { showSend = false }, onSent = { showSend = false; refreshKey++ }) }
    if (showBeamSend) BeamSendSheet(onClose = { showBeamSend = false }, onSent = { showBeamSend = false; refreshKey++ })
    if (showElaSend) elaAddr?.let { ElaSendSheet(onClose = { showElaSend = false }, onSent = { showElaSend = false; refreshKey++ }) }
    showTokens?.let { c -> TokenSheet(c) { showTokens = null } }
    showNfts?.let { c -> NftGridSheet(c) { showNfts = null } }
    if (showBeamAssets) BeamAssetSheet(
        onReceive = { showBeamAssets = false; showReceive = chains.firstOrNull { it.key == "beam" } },
        onSend = { showBeamAssets = false; showBeamSend = true },
        onClose = { showBeamAssets = false },
    )
    if (showSettings) WalletSettingsSheet { showSettings = false }
}

@Composable
private fun TxRow(t: TxRecord) {
    Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(if (t.kind == "tip") Icons.Filled.Paid else Icons.AutoMirrored.Filled.Send, null, tint = goldInk, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text("${if (t.kind == "tip") "Tipped" else "Sent"} ${t.amount} ${t.symbol}", color = ink, fontSize = 14.sp, fontWeight = FontWeight.Medium)
            Text("to ${shortAddr(t.to)} · ${relativeTime(t.ts)}", color = muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    androidx.compose.material3.HorizontalDivider(color = glassBorder)
}

/** Wallet settings (the gear): show/hide transaction history + how the BEAM (private)
 *  wallet syncs (quick-sync via a public node, or a self-hosted on-device light node). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WalletSettingsSheet(onClose: () -> Unit) {
    val ctx = LocalContext.current
    var showHist by remember { mutableStateOf(HeyApi.showTxHistory(ctx)) }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("Wallet settings", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(16.dp))
            Row(Modifier.fillMaxWidth().glass(14.dp).padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.Receipt, null, tint = goldInk, modifier = Modifier.size(22.dp))
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text("Show transaction history", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                    Text("Your sends + tips (received payments coming soon).", color = muted, fontSize = 12.sp)
                }
                Switch(checked = showHist, onCheckedChange = { showHist = it; HeyApi.setShowTxHistory(ctx, it) },
                    colors = SwitchDefaults.colors(checkedThumbColor = Navy, checkedTrackColor = Gold))
            }
            Spacer(Modifier.height(14.dp))
            Column(Modifier.fillMaxWidth().glass(14.dp).padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Shield, null, tint = goldInk, modifier = Modifier.size(22.dp))
                    Spacer(Modifier.width(10.dp))
                    Text("BEAM private wallet", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                }
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Bolt, null, tint = goldInk, modifier = Modifier.size(14.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Quick sync — light FlyClient verification against your configured BEAM node: a public mainnet node by default, or self-host your own (below). Nothing runs on your phone either way.", color = muted, fontSize = 12.sp)
                }
                Spacer(Modifier.height(12.dp))
                // Self-host the BEAM node RIGHT HERE — quick-sync + send use it (else the
                // public mainnet default). Run your own beam-node and paste host:port.
                RpcNodeRow(
                    RpcNode(
                        "beam", "BEAM node (self-host)", BeamApi.DEFAULT_NODE,
                        HeyApi.beamNode(ctx).let { if (it == BeamApi.DEFAULT_NODE) "" else it },
                    ),
                ) { HeyApi.setBeamNode(ctx, it); "{\"ok\":true}" } // BEAM = raw host:port, no H6 https gate
                Spacer(Modifier.height(14.dp))
                Divider(color = glassBorder)
                Spacer(Modifier.height(12.dp))
                // Money-safety gate: first sends are sub-cent-capped; lift only after a proven test send.
                var capLifted by remember { mutableStateOf(HeyApi.beamCapLifted(ctx)) }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Lift the send safety cap", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                        Text("First sends are limited to ${BeamApi.SEND_CAP_BEAM} BEAM. Turn this on only AFTER a successful test send.", color = muted, fontSize = 11.sp)
                    }
                    Spacer(Modifier.width(8.dp))
                    Switch(checked = capLifted, onCheckedChange = { capLifted = it; HeyApi.setBeamCapLifted(ctx, it) },
                        colors = SwitchDefaults.colors(checkedThumbColor = Navy, checkedTrackColor = Gold))
                }
            }
            Spacer(Modifier.height(14.dp))
            // ── Blockchain nodes (self-host RPC; default = bundled public Elastos RPC) ──
            Column(Modifier.fillMaxWidth().glass(14.dp).padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Dns, null, tint = goldInk, modifier = Modifier.size(22.dp))
                    Spacer(Modifier.width(10.dp))
                    Text("Blockchain nodes", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                }
                Spacer(Modifier.height(4.dp))
                Text("Default: public Elastos RPC. Self-host: point a chain at your own node.", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(12.dp))
                // Snapshot the current overrides once (read_untracked-style): editing a field
                // shouldn't reset the others. key/name/default/override per chain from Rust.
                val rpcNodes = remember {
                    val arr = HeyApi.rpcNodes()
                    (0 until arr.length()).map { arr.getJSONObject(it) }.map {
                        RpcNode(it.optString("key"), it.optString("name"), it.optString("default"), it.optString("override"), it.optBoolean("insecure"))
                    }
                }
                rpcNodes.forEachIndexed { i, n ->
                    if (i > 0) Spacer(Modifier.height(10.dp))
                    RpcNodeRow(n) { HeyApi.setRpcNode(n.key, it) }
                }
                // (BEAM's self-host node field lives in the BEAM card above + the BEAM wallet sheet.)
            }
            Spacer(Modifier.height(20.dp))
        }
    }
}

/** One self-hostable chain: bundled `default` endpoint + the current `override` ("" = on default).
 *  `insecure` = the override is a tolerated cleartext (http loopback/LAN) node (H6). */
private data class RpcNode(val key: String, val name: String, val default: String, val override: String, val insecure: Boolean = false)

/** A single "Blockchain nodes" row: chain label + a field prefilled with the override
 *  (placeholder = the public default). Empty on save → revert to the default. Shows a
 *  tiny "using default" vs "self-hosted" indicator, plus an INSECURE badge for a
 *  cleartext (http) loopback/LAN node. */
@Composable
private fun RpcNodeRow(node: RpcNode, onSaveRes: (String) -> String) {
    var value by remember(node.key) { mutableStateOf(node.override) }
    var selfHosted by remember(node.key) { mutableStateOf(node.override.isNotBlank()) }
    var insecure by remember(node.key) { mutableStateOf(node.insecure) }
    var err by remember(node.key) { mutableStateOf("") }
    Column(Modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(node.name, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, modifier = Modifier.weight(1f))
            if (insecure) {
                Text("INSECURE (http)", color = Like, fontSize = 10.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.width(6.dp))
            }
            Text(
                if (selfHosted) "self-hosted" else "using default",
                color = if (selfHosted) goldInk else muted, fontSize = 10.sp,
            )
        }
        Spacer(Modifier.height(4.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = value, onValueChange = { value = it },
                placeholder = { Text(node.default, color = muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                singleLine = true, colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink, fontSize = 12.sp, fontFamily = mono),
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            TextButton(onClick = {
                val v = value.trim(); value = v
                // Rust validates (H6): a money/signing chain rejects a cleartext PUBLIC
                // node ({"error":…}); a tolerated http loopback/LAN node returns ok and
                // we surface the INSECURE badge. https / cleared → ok, no badge.
                val res = runCatching { org.json.JSONObject(onSaveRes(v)) }.getOrNull()
                val errMsg = res?.optString("error").orEmpty()
                if (errMsg.isNotBlank()) {
                    err = errMsg
                } else {
                    err = ""
                    selfHosted = v.isNotBlank()
                    insecure = v.startsWith("http://", ignoreCase = true)
                }
            }) { Text("Save", color = goldInk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold) }
        }
        if (err.isNotBlank()) { Spacer(Modifier.height(4.dp)); Text(err, color = Like, fontSize = 11.sp) }
    }
}

@Composable
private fun ModeOption(title: String, body: String, value: String, selected: String, enabled: Boolean = true, onSelect: () -> Unit) {
    val on = selected == value
    val a = if (enabled) 1f else 0.4f
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
            .background(if (on) Gold.copy(alpha = 0.16f) else Color.Transparent)
            .border(1.dp, if (on) goldInk else glassBorder, RoundedCornerShape(12.dp))
            .then(if (enabled) Modifier.clickable { onSelect() } else Modifier).padding(12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(if (on) Icons.Filled.RadioButtonChecked else Icons.Filled.RadioButtonUnchecked, null, tint = (if (on) goldInk else muted).copy(alpha = a), modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = ink.copy(alpha = a), fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            Text(body, color = muted.copy(alpha = a), fontSize = 12.sp)
        }
    }
}

/** Tokens on one EVM chain: native + curated ERC-20s, with hide (scam protection)
 *  and tap-to-send. Curated list, so random airdropped scam tokens never appear. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TokenSheet(chain: UiChain, onClose: () -> Unit) {
    val ctx = LocalContext.current
    var tokens by remember { mutableStateOf<List<TokenBal>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var showHidden by remember { mutableStateOf(false) }
    var refresh by remember { mutableStateOf(0) }
    var sendTok by remember { mutableStateOf<TokenBal?>(null) }
    val hiddenN = remember(refresh) { HeyApi.hiddenCount(ctx, chain.key) }
    LaunchedEffect(refresh, showHidden) {
        loading = true
        tokens = withContext(Dispatchers.IO) { runCatching { HeyApi.balances(ctx, chain.key, includeHidden = showHidden) }.getOrDefault(emptyList()) }
        loading = false
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("${chain.title} tokens", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Text("Tap a token to send it. Only verified tokens are shown.", color = muted, fontSize = 12.sp)
            Spacer(Modifier.height(14.dp))
            if (loading && tokens.isEmpty()) {
                Box(Modifier.fillMaxWidth().padding(20.dp), Alignment.Center) { CircularProgressIndicator(color = goldInk) }
            } else {
                tokens.forEach { t ->
                    val hidden = !t.native && HeyApi.isTokenHidden(ctx, chain.key, t.contract)
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { sendTok = t }.padding(vertical = 10.dp, horizontal = 4.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Box(Modifier.size(36.dp).clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                            Text(t.symbol.take(1), color = Navy, fontWeight = FontWeight.Bold)
                        }
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(t.symbol, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                            Text(if (t.native) "${chain.title} · native" else t.name, color = muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        Text(t.balance, color = ink, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                        if (!t.native) {
                            Spacer(Modifier.width(4.dp))
                            IconButton(onClick = { HeyApi.setTokenHidden(ctx, chain.key, t.contract, !hidden); refresh++ }) {
                                Icon(if (hidden) Icons.Filled.Visibility else Icons.Filled.VisibilityOff,
                                    if (hidden) "Unhide" else "Hide (scam protection)", tint = muted, modifier = Modifier.size(20.dp))
                            }
                        }
                    }
                    androidx.compose.material3.HorizontalDivider(color = glassBorder)
                }
                if (hiddenN > 0) {
                    Spacer(Modifier.height(6.dp))
                    TextButton(onClick = { showHidden = !showHidden }) {
                        Icon(if (showHidden) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, null, Modifier.size(16.dp), tint = muted)
                        Spacer(Modifier.width(6.dp))
                        Text(if (showHidden) "Hide hidden tokens" else "Show $hiddenN hidden", color = muted, fontSize = 13.sp)
                    }
                }
                Spacer(Modifier.height(8.dp))
                Text("Tokens you didn't ask for? Hide them — a scammer can airdrop a fake token, but it can't move your funds.", color = muted, fontSize = 11.sp)
            }
            Spacer(Modifier.height(16.dp))
        }
    }
    sendTok?.let { t ->
        SendSheet(
            chain = chain.key, symbol = chain.symbol, network = chain.title,
            token = if (t.native) null else t,
            onClose = { sendTok = null }, onSent = { sendTok = null; onClose() },
        )
    }
}

/** Entry card for the wallet's collectibles surface (EVM chains only). Shows a
 *  small preview strip of the first few NFTs + a count; tap → NftGridSheet. */
@Composable
private fun CollectiblesCard(chain: UiChain, addr: String?, onClick: () -> Unit) {
    val ctx = LocalContext.current
    var preview by remember(chain.key) { mutableStateOf<List<HeyNft>?>(null) }
    LaunchedEffect(chain.key, addr) {
        preview = withContext(Dispatchers.IO) {
            runCatching { HeyApi.nfts(ctx, chain.key).items }.getOrDefault(emptyList())
        }
    }
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).glass(18.dp).clickable { onClick() }.padding(16.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Collections, null, tint = goldInk, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text("Collectibles", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            Spacer(Modifier.weight(1f))
            val n = preview?.size
            Text(
                when { n == null -> "…"; n == 0 -> "none yet"; else -> "$n" },
                color = muted, fontSize = 12.sp
            )
            Spacer(Modifier.width(4.dp))
            Icon(Icons.Filled.ChevronRight, null, tint = muted, modifier = Modifier.size(18.dp))
        }
        preview?.takeIf { it.isNotEmpty() }?.let { list ->
            Spacer(Modifier.height(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                list.take(4).forEach { nft ->
                    Box(Modifier.size(56.dp).clip(RoundedCornerShape(12.dp)).background(glassFill)) {
                        if (nft.image.isNotBlank()) AsyncImage(
                            model = nftImageModel(nft.image), contentDescription = null,
                            contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()
                        )
                    }
                }
            }
        }
    }
}

/** The wallet's Collectibles grid — every NFT (721/1155) the wallet owns on this
 *  EVM chain. Mirrors TokenSheet's load/refresh/empty patterns. Per-tile hide
 *  (scam-airdrop defense), an honest discovery-mode label, and "+ Add collection"
 *  (manual tracking for the indexer-off / trustless mode). Tap a tile → detail. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NftGridSheet(chain: UiChain, onClose: () -> Unit) {
    val ctx = LocalContext.current
    var list by remember { mutableStateOf<NftList?>(null) }
    var loading by remember { mutableStateOf(true) }
    var showHidden by remember { mutableStateOf(false) }
    var refresh by remember { mutableStateOf(0) }
    var detail by remember { mutableStateOf<HeyNft?>(null) }
    var addDlg by remember { mutableStateOf(false) }
    val hiddenN = remember(refresh) { HeyApi.hiddenNftCount(ctx, chain.key) }
    LaunchedEffect(refresh, showHidden) {
        loading = true
        list = withContext(Dispatchers.IO) {
            runCatching { HeyApi.nfts(ctx, chain.key, includeHidden = showHidden) }.getOrDefault(NftList("tracked", emptyList()))
        }
        loading = false
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).heightIn(max = 600.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Collectibles · ${chain.title}", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                    val tracked = list?.mode == "tracked"
                    Text(
                        if (tracked) "Tracked collections (your NFT index is off — only collections you add are shown)."
                        else "Tap a collectible for details. Hide any you didn't ask for.",
                        color = muted, fontSize = 12.sp
                    )
                }
                IconButton(onClick = { addDlg = true }) { Icon(Icons.Filled.Add, "Add a collection to track", tint = goldInk) }
            }
            Spacer(Modifier.height(14.dp))
            val items = list?.items ?: emptyList()
            when {
                loading && items.isEmpty() -> Box(Modifier.fillMaxWidth().padding(30.dp), Alignment.Center) { CircularProgressIndicator(color = goldInk) }
                items.isEmpty() -> Text(
                    "No collectibles on ${chain.title} yet — anything you collect (e.g. on ela.city) will appear here.",
                    color = muted, fontSize = 13.sp
                )
                else -> LazyVerticalGrid(columns = GridCells.Fixed(2), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.weight(1f, fill = false)) {
                    gridItems(items) { nft ->
                        Box(
                            Modifier.aspectRatio(1f).clip(RoundedCornerShape(16.dp)).background(glassFill).clickable { detail = nft }
                        ) {
                            if (nft.image.isNotBlank()) AsyncImage(
                                model = nftImageModel(nft.image), contentDescription = nft.name,
                                contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()
                            ) else Box(Modifier.fillMaxSize(), Alignment.Center) {
                                Icon(Icons.Filled.Image, null, tint = muted, modifier = Modifier.size(34.dp))
                            }
                            // name overlay
                            Box(Modifier.align(Alignment.BottomStart).fillMaxWidth()
                                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.55f))))
                                .padding(8.dp)) {
                                Text(nft.name.ifBlank { "#${nft.tokenId}" }, color = Color.White, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            if (nft.is1155) Box(Modifier.align(Alignment.TopEnd).padding(6.dp).clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.55f)).padding(horizontal = 6.dp, vertical = 2.dp)) {
                                Text("×${nft.amount}", color = Color.White, fontSize = 10.sp, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                }
            }
            if (hiddenN > 0) {
                Spacer(Modifier.height(6.dp))
                TextButton(onClick = { showHidden = !showHidden }) {
                    Icon(if (showHidden) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, null, Modifier.size(16.dp), tint = muted)
                    Spacer(Modifier.width(6.dp))
                    Text(if (showHidden) "Hide hidden" else "Show $hiddenN hidden", color = muted, fontSize = 13.sp)
                }
            }
            Spacer(Modifier.height(16.dp))
        }
    }
    detail?.let { nft ->
        NftDetailSheet(chain = chain, nft = nft, onHide = { HeyApi.setNftHidden(ctx, chain.key, nft.contract, nft.tokenId, true); detail = null; refresh++ }, onSent = { detail = null; onClose() }, onClose = { detail = null })
    }
    if (addDlg) {
        var contract by remember { mutableStateOf("") }
        var err by remember { mutableStateOf("") }
        var busy by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
        AlertDialog(
            onDismissRequest = { if (!busy) addDlg = false },
            icon = { Icon(Icons.Filled.Add, null, tint = goldInk) },
            title = { Text("Track a collection", color = ink) },
            text = {
                Column {
                    Text("Paste an NFT contract address (0x…) to track it — useful when the index is off, or for a collection the index missed.", color = muted, fontSize = 12.sp)
                    Spacer(Modifier.height(10.dp))
                    OutlinedTextField(value = contract, onValueChange = { contract = it; err = "" }, singleLine = true,
                        label = { Text("Contract (0x…)") }, textStyle = androidx.compose.ui.text.TextStyle(fontFamily = mono, fontSize = 13.sp),
                        modifier = Modifier.fillMaxWidth(), colors = glassFieldColors())
                    if (err.isNotBlank()) { Spacer(Modifier.height(8.dp)); Text(err, color = Like, fontSize = 12.sp) }
                }
            },
            confirmButton = {
                TextButton(enabled = !busy, onClick = {
                    busy = true
                    scope.launch {
                        val res = withContext(Dispatchers.IO) { HeyApi.checkAddress(contract) }
                        busy = false
                        res.onSuccess { HeyApi.addPinnedNftCollection(ctx, chain.key, it); addDlg = false; refresh++ }
                            .onFailure { err = it.message ?: "Invalid contract address" }
                    }
                }) { Text("Track", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { if (!busy) addDlg = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

/** One collectible in detail: large image, collection/contract/id/standard, and
 *  the actions — Send, hide, view on the explorer. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NftDetailSheet(chain: UiChain, nft: HeyNft, onHide: () -> Unit, onSent: () -> Unit, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var send by remember { mutableStateOf(false) }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Box(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(20.dp)).background(glassFill)) {
                if (nft.image.isNotBlank()) AsyncImage(model = nftImageModel(nft.image), contentDescription = nft.name, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize())
                else Box(Modifier.fillMaxSize(), Alignment.Center) { Icon(Icons.Filled.Image, null, tint = muted, modifier = Modifier.size(54.dp)) }
            }
            Spacer(Modifier.height(14.dp))
            Text(nft.name.ifBlank { "#${nft.tokenId}" }, color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            if (nft.collection.isNotBlank()) Text(nft.collection, color = muted, fontSize = 13.sp)
            Spacer(Modifier.height(12.dp))
            NftDetailRow("Token ID", nft.tokenId)
            NftDetailRow("Standard", if (nft.is1155) "ERC-1155 (×${nft.amount} owned)" else "ERC-721")
            Row(Modifier.fillMaxWidth().clickable {
                clipboard.setText(AnnotatedString(nft.contract))
                android.widget.Toast.makeText(ctx, "Contract copied", android.widget.Toast.LENGTH_SHORT).show()
            }.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Contract", color = muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
                Text(shortAddr(nft.contract), color = ink, fontSize = 13.sp, fontFamily = mono)
                Spacer(Modifier.width(6.dp)); Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(14.dp))
            }
            Spacer(Modifier.height(16.dp))
            Button(onClick = { send = true }, modifier = Modifier.fillMaxWidth().height(50.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Send", fontWeight = FontWeight.Bold)
            }
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedButton(onClick = {
                    runCatching {
                        val url = "https://esc.elastos.io/token/${nft.contract}/instance/${nft.tokenId}"
                        ctx.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(url)))
                    }
                }, modifier = Modifier.weight(1f)) { Icon(Icons.Filled.OpenInNew, null, Modifier.size(16.dp), tint = ink); Spacer(Modifier.width(6.dp)); Text("Explorer", color = ink) }
                OutlinedButton(onClick = onHide, modifier = Modifier.weight(1f)) { Icon(Icons.Filled.VisibilityOff, null, Modifier.size(16.dp), tint = ink); Spacer(Modifier.width(6.dp)); Text("Hide", color = ink) }
            }
            Spacer(Modifier.height(16.dp))
        }
    }
    if (send) NftSendSheet(chain = chain, nft = nft, onClose = { send = false }, onSent = { send = false; onSent() })
}

@Composable
private fun NftDetailRow(label: String, value: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
        Text(value, color = ink, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Send an NFT — clones SendSheet's proven money flow (review→checkAddress, the
 *  SpendAuth.spendGrant pipeline bound to the SAME canonical (kind,to,amount) the
 *  Rust signer redeems, requireAuth + SecureWindow on the confirm, txHash→txStatus
 *  poll, recordTx). The confirm shows the NFT image + "Send {name} (#{id})"
 *  (+ "× {qty}" for 1155). For 1155 the quantity is bound into the grant kind so
 *  confirming "send #5" can't move a different count. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NftSendSheet(chain: UiChain, nft: HeyNft, onClose: () -> Unit, onSent: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val activity = ctx as? androidx.fragment.app.FragmentActivity
    var to by remember { mutableStateOf("") }
    var qty by remember { mutableStateOf(if (nft.is1155) "1" else "1") }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf(false) }
    var txHash by remember { mutableStateOf<String?>(null) }
    val scanner = rememberLauncherForActivityResult(ScanContract()) { r ->
        r.contents?.let { to = it.trim().removePrefix("ethereum:").substringBefore("?").substringBefore("@") }
    }

    fun doSend() {
        busy = true; status = "Authorizing…"
        scope.launch {
            // Spend grant bound to the SAME canonical (kind,to,amount) the signer
            // redeems. amount = the DECIMAL token_id verbatim; 1155 binds the qty.
            val grant = if (nft.is1155)
                SpendAuth.spendGrant(activity, "nft1155:${chain.key}:${nft.contract}:${qty.trim()}", to.trim(), nft.tokenId) {
                    HeyApi.authorizeNftSend1155(chain.key, nft.contract, to, nft.tokenId, qty) }
            else
                SpendAuth.spendGrant(activity, "nft:${chain.key}:${nft.contract}", to.trim(), nft.tokenId) {
                    HeyApi.authorizeNftSend721(chain.key, nft.contract, to, nft.tokenId) }
            if (grant == null) { busy = false; status = "Authorization cancelled"; return@launch }
            status = "Signing & broadcasting…"
            val res = withContext(Dispatchers.IO) {
                if (nft.is1155) HeyApi.nftSend1155(ctx, chain.key, nft.contract, to, nft.tokenId, qty, grant)
                else HeyApi.nftSend721(ctx, chain.key, nft.contract, to, nft.tokenId, grant)
            }
            busy = false
            res.onSuccess { txHash = it; status = ""; HeyApi.recordTx(ctx, chain.key, nft.name.ifBlank { "NFT" }, to, if (nft.is1155) "#${nft.tokenId}×$qty" else "#${nft.tokenId}", it, "nft") }
                .onFailure { status = it.message ?: "Send failed" }
        }
    }
    fun review() {
        if (nft.is1155) {
            val q = qty.trim().toLongOrNull()
            if (q == null || q <= 0) { status = "Enter a quantity"; return }
        }
        busy = true; status = "Checking address…"
        scope.launch {
            val res = withContext(Dispatchers.IO) { HeyApi.checkAddress(to) }
            busy = false
            res.onSuccess { to = it; status = ""; confirm = true }
                .onFailure { status = it.message ?: "Invalid recipient address" }
        }
    }

    ModalBottomSheet(onDismissRequest = { if (!busy) onClose() }, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            if (txHash != null) {
                var confState by remember { mutableStateOf("pending") }
                LaunchedEffect(txHash) {
                    repeat(24) {
                        kotlinx.coroutines.delay(3000)
                        val s = withContext(Dispatchers.IO) { HeyApi.txStatus(chain.key, txHash!!) }
                        if (s == "success" || s == "failed") { confState = s; return@LaunchedEffect }
                    }
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    when (confState) {
                        "success" -> Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(56.dp))
                        "failed" -> Icon(Icons.Filled.Error, null, tint = Like, modifier = Modifier.size(56.dp))
                        else -> CircularProgressIndicator(color = goldInk, modifier = Modifier.size(48.dp), strokeWidth = 3.dp)
                    }
                    Spacer(Modifier.height(12.dp))
                    Text(when (confState) { "success" -> "Confirmed"; "failed" -> "Failed on-chain"; else -> "Broadcast" }, color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(6.dp))
                    Text(
                        when (confState) {
                            "success" -> "Your collectible is on its way."
                            "failed" -> "The transaction reverted on-chain — gas was spent but the NFT was NOT sent. Re-check the recipient and try again."
                            else -> "Sent to the network — confirming on-chain…"
                        }, color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center
                    )
                    Spacer(Modifier.height(12.dp))
                    Row(Modifier.clip(RoundedCornerShape(10.dp)).clickable {
                        clipboard.setText(AnnotatedString(txHash!!))
                        android.widget.Toast.makeText(ctx, "Transaction hash copied", android.widget.Toast.LENGTH_SHORT).show()
                    }.padding(8.dp, 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("tx ${shortAddr(txHash!!)}", color = goldInk, fontSize = 12.sp, fontFamily = mono)
                        Spacer(Modifier.width(6.dp)); Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(13.dp))
                    }
                    Spacer(Modifier.height(20.dp))
                    Button(onClick = onSent, modifier = Modifier.fillMaxWidth(), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) { Text("Done", fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.height(16.dp))
                }
            } else {
                Text("Send ${nft.name.ifBlank { "#${nft.tokenId}" }}", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                Text("On ${chain.title}", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(18.dp))
                OutlinedTextField(
                    value = to, onValueChange = { to = it; status = "" }, singleLine = true,
                    label = { Text("Recipient address (0x…)") },
                    trailingIcon = { IconButton(onClick = { scanner.launch(scanOptions()) }) { Icon(Icons.Filled.QrCodeScanner, "Scan", tint = goldInk) } },
                    textStyle = androidx.compose.ui.text.TextStyle(fontFamily = mono, fontSize = 13.sp),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                if (nft.is1155) {
                    Spacer(Modifier.height(12.dp))
                    OutlinedTextField(
                        value = qty, onValueChange = { qty = it.filter { c -> c.isDigit() }; status = "" }, singleLine = true,
                        label = { Text("Quantity (you own ${nft.amount})") },
                        keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number),
                        modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                    )
                }
                Spacer(Modifier.height(14.dp))
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Gold.copy(alpha = 0.10f)).padding(12.dp), verticalAlignment = Alignment.Top) {
                    Icon(Icons.Filled.Info, null, tint = goldInk, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("This transfers a real collectible and can't be undone. Double-check the address.", color = muted, fontSize = 12.sp)
                }
                if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = Like, fontSize = 13.sp) }
                Spacer(Modifier.height(18.dp))
                Button(onClick = { review() }, enabled = !busy, modifier = Modifier.fillMaxWidth().height(50.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                    else { Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Review & send", fontWeight = FontWeight.Bold) }
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }

    if (confirm) {
        AlertDialog(
            onDismissRequest = { if (!busy) confirm = false },
            icon = { Icon(Icons.AutoMirrored.Filled.Send, null, tint = goldInk) },
            title = { Text("Confirm transfer", color = ink) },
            text = {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    SecureWindow() // block screenshots + tap-jacking on the money confirm
                    Box(Modifier.size(96.dp).clip(RoundedCornerShape(14.dp)).background(glassFill)) {
                        if (nft.image.isNotBlank()) AsyncImage(model = nftImageModel(nft.image), contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                        else Box(Modifier.fillMaxSize(), Alignment.Center) { Icon(Icons.Filled.Image, null, tint = muted, modifier = Modifier.size(34.dp)) }
                    }
                    Spacer(Modifier.height(10.dp))
                    Text(
                        "Send ${nft.name.ifBlank { "NFT" }} (#${nft.tokenId})" + if (nft.is1155) " × ${qty.trim()}" else "",
                        color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold, textAlign = androidx.compose.ui.text.style.TextAlign.Center
                    )
                    Spacer(Modifier.height(4.dp))
                    Text("to ${shortAddr(to)}", color = muted, fontSize = 13.sp, fontFamily = mono)
                    Spacer(Modifier.height(10.dp))
                    Text("This signs with your key and broadcasts on ${chain.title}. It cannot be reversed.", color = muted, fontSize = 12.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                }
            },
            confirmButton = {
                TextButton(enabled = !busy, onClick = { spendGate(activity, ctx) { confirm = false; doSend() } }) { Text("Sign & send", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { if (!busy) confirm = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

/** A chain shown as a card in the wallet stack. `evm` = full send+balance today
 *  (ESC/Ethereum/…); the Elastos mainchain is receive-only for now. */
private data class UiChain(val key: String, val title: String, val sub: String, val evm: Boolean, val symbol: String)

@Composable
private fun ChainCard(
    chain: UiChain, address: String?, balance: String?, loading: Boolean,
    onClick: () -> Unit, onCopy: (String) -> Unit,
) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp))
            .background(Brush.verticalGradient(listOf(Gold.copy(alpha = 0.22f), Gold.copy(alpha = 0.06f))))
            .border(1.dp, glassBorder, RoundedCornerShape(22.dp)).clickable { onClick() }.padding(22.dp)
    ) {
        if (chain.evm || chain.key == "beam") {
            Text(if (chain.evm) "Tap to view tokens" else "Tap to view assets", color = muted, fontSize = 10.sp, modifier = Modifier.align(Alignment.End))
            Spacer(Modifier.height(2.dp))
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(if (chain.evm) Icons.Filled.Bolt else Icons.Filled.Link, null, tint = goldInk, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
            Text(chain.title, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            Spacer(Modifier.weight(1f))
            Text(chain.sub, color = muted, fontSize = 11.sp)
        }
        Spacer(Modifier.height(16.dp))
        Text("Balance", color = muted, fontSize = 12.sp)
        Spacer(Modifier.height(4.dp))
        if (loading && balance == null) {
            CircularProgressIndicator(color = goldInk, modifier = Modifier.size(26.dp), strokeWidth = 3.dp)
        } else {
            Row(verticalAlignment = Alignment.Bottom) {
                Text(balance ?: "—", color = ink, fontSize = 38.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.width(8.dp))
                Text(chain.symbol, color = goldInk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(bottom = 5.dp))
            }
        }
        Spacer(Modifier.height(16.dp))
        if (address != null) {
            Row(
                Modifier.clip(RoundedCornerShape(12.dp)).background(Color.Black.copy(alpha = 0.13f))
                    .clickable { onCopy(address) }.padding(12.dp, 9.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(shortAddr(address), color = ink, fontSize = 13.sp, fontFamily = mono)
                Spacer(Modifier.width(8.dp))
                Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(15.dp))
            }
        } else {
            Text("Deriving…", color = muted, fontSize = 12.sp)
        }
    }
}

/** Reusable elegant generation animation: a pulsing gold seal + sequentially
 *  ticking steps over ~totalMs, then onDone(). The underlying keys are derived
 *  instantly; this is choreography so setup feels considered. */
@Composable
private fun GeneratingSteps(
    title: String,
    subtitle: String,
    steps: List<Pair<String, androidx.compose.ui.graphics.vector.ImageVector>>,
    totalMs: Long = 2600,
    onDone: () -> Unit,
) {
    var step by remember { mutableStateOf(0) }
    val infinite = rememberInfiniteTransition(label = "gen")
    val pulse by infinite.animateFloat(
        initialValue = 1f, targetValue = 1.10f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "pulse"
    )
    LaunchedEffect(Unit) {
        val per = (totalMs / steps.size).coerceAtLeast(250)
        for (i in steps.indices) { kotlinx.coroutines.delay(per); step = i + 1 }
        kotlinx.coroutines.delay(260); onDone()
    }
    Column(
        Modifier.fillMaxSize().padding(32.dp), Arrangement.Center, Alignment.CenterHorizontally
    ) {
        Box(
            Modifier.size(108.dp).graphicsLayer { scaleX = pulse; scaleY = pulse }
                .clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))),
            Alignment.Center
        ) { Icon(Icons.Filled.Fingerprint, null, tint = Navy, modifier = Modifier.size(52.dp)) }
        Spacer(Modifier.height(24.dp))
        Text(title, color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        Spacer(Modifier.height(6.dp))
        Text(subtitle, color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        Spacer(Modifier.height(28.dp))
        Column(Modifier.fillMaxWidth().glass(18.dp).padding(18.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            steps.forEachIndexed { i, (label, icon) ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(24.dp), Alignment.Center) {
                        when {
                            i < step -> Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(22.dp))
                            i == step -> CircularProgressIndicator(color = goldInk, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                            else -> Icon(icon, null, tint = muted.copy(alpha = 0.5f), modifier = Modifier.size(20.dp))
                        }
                    }
                    Spacer(Modifier.width(12.dp))
                    Text(label, color = if (i <= step) ink else muted, fontSize = 14.sp,
                        fontWeight = if (i == step) FontWeight.SemiBold else FontWeight.Normal)
                }
            }
        }
    }
}

/** First-run elegant generation: the did:elastos derivation is instant (local
 *  P-256), but we choreograph the steps so it feels like a real, considered setup,
 *  then reveal the user's new Elastos DID. */
@Composable
private fun DidGenerationScreen(onDone: () -> Unit) {
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val steps = listOf(
        "Deriving your keys" to Icons.Filled.Key,
        "Creating your Elastos DID" to Icons.Filled.Badge,
        "Setting up your wallets" to Icons.Filled.AccountBalanceWallet,
        "Securing on this device" to Icons.Filled.Shield,
    )
    var step by remember { mutableStateOf(0) }
    var did by remember { mutableStateOf<String?>(null) }
    var revealed by remember { mutableStateOf(false) }

    val infinite = rememberInfiniteTransition(label = "gen")
    val pulse by infinite.animateFloat(
        initialValue = 1f, targetValue = 1.10f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse), label = "pulse"
    )

    LaunchedEffect(Unit) {
        val derived = withContext(Dispatchers.IO) { runCatching { HeyApi.elastosDid(ctx) }.getOrNull() }
        for (i in steps.indices) { kotlinx.coroutines.delay(620); step = i + 1 }
        kotlinx.coroutines.delay(280)
        did = derived
        revealed = true
    }

    Column(
        Modifier.fillMaxSize().padding(32.dp, 0.dp, 32.dp, 110.dp),
        verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally
    ) {
        if (!revealed) {
            Box(
                Modifier.size(108.dp).graphicsLayer { scaleX = pulse; scaleY = pulse }
                    .clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))),
                Alignment.Center
            ) { Icon(Icons.Filled.Fingerprint, null, tint = Navy, modifier = Modifier.size(52.dp)) }
            Spacer(Modifier.height(24.dp))
            Text("Creating your Elastos identity", color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text("One DID for your wallets — derived on this device, in a second.",
                color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Spacer(Modifier.height(28.dp))
            Column(Modifier.fillMaxWidth().glass(18.dp).padding(18.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                steps.forEachIndexed { i, (label, icon) ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(24.dp), Alignment.Center) {
                            when {
                                i < step -> Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(22.dp))
                                i == step -> CircularProgressIndicator(color = goldInk, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                                else -> Icon(icon, null, tint = muted.copy(alpha = 0.5f), modifier = Modifier.size(20.dp))
                            }
                        }
                        Spacer(Modifier.width(12.dp))
                        Text(label, color = if (i <= step) ink else muted, fontSize = 14.sp,
                            fontWeight = if (i == step) FontWeight.SemiBold else FontWeight.Normal)
                    }
                }
            }
        } else {
            Box(Modifier.size(96.dp).clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                Icon(Icons.Filled.Verified, null, tint = Navy, modifier = Modifier.size(48.dp))
            }
            Spacer(Modifier.height(22.dp))
            Text("Your Elastos DID is ready", color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(10.dp))
            did?.let { d ->
                Row(
                    Modifier.clip(RoundedCornerShape(12.dp)).background(Color.Black.copy(alpha = 0.10f))
                        .clickable {
                            clipboard.setText(AnnotatedString(d))
                            android.widget.Toast.makeText(ctx, "DID copied", android.widget.Toast.LENGTH_SHORT).show()
                        }.padding(12.dp, 10.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(d.removePrefix("did:elastos:").let { "did:elastos:${it.take(8)}…${it.takeLast(6)}" },
                        color = goldInk, fontSize = 13.sp, fontFamily = mono)
                    Spacer(Modifier.width(8.dp)); Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(14.dp))
                }
            }
            Spacer(Modifier.height(14.dp))
            Text("Recover it anytime in official Elastos Essentials with your recovery phrase. It manages your ELA + ESC wallets.",
                color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center, lineHeight = 19.sp)
            Spacer(Modifier.height(28.dp))
            Button(onClick = onDone, modifier = Modifier.fillMaxWidth().height(52.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                Text("Enter wallet", fontWeight = FontWeight.Bold, fontSize = 16.sp)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReceiveSheet(address: String, chainTitle: String, chainSub: String, symbol: String, onClose: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    var qr by remember { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(address) { withContext(Dispatchers.IO) { runCatching { qr = qrBitmap(address) } } }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
            Text("Receive $symbol", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text("Scan or copy to receive $symbol on $chainTitle ($chainSub). Only send $symbol on this network to this address.", color = muted, fontSize = 12.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Spacer(Modifier.height(16.dp))
            Box(Modifier.fillMaxWidth(0.78f).aspectRatio(1f).clip(RoundedCornerShape(16.dp)).background(Color.White).padding(10.dp), Alignment.Center) {
                if (qr != null) Image(qr!!.asImageBitmap(), "Address QR", Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                else CircularProgressIndicator(color = Navy)
            }
            Spacer(Modifier.height(14.dp))
            Text(address, color = ink, fontSize = 13.sp, fontFamily = mono, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { shareText(ctx, address) }, colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    Icon(Icons.Filled.Share, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Share", fontWeight = FontWeight.Bold)
                }
                OutlinedButton(onClick = {
                    clipboard.setText(AnnotatedString(address))
                    android.widget.Toast.makeText(ctx, "Address copied", android.widget.Toast.LENGTH_SHORT).show()
                }) { Icon(Icons.Filled.ContentCopy, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Copy", color = ink) }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** BEAM assets sheet — tap the BEAM card. Shows node mode + sync status and the BEAM (asset 0) +
 *  BEAMX (asset 7) balances. Sync connects to the node (quicksync) and refreshes. Send is next. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BeamAssetSheet(onReceive: () -> Unit, onSend: () -> Unit, onClose: () -> Unit) {
    val ctx = LocalContext.current
    var bal by remember { mutableStateOf<BeamBalance?>(null) }
    var syncing by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    var progress by remember { mutableStateOf<BeamSyncProgress?>(null) }
    var linkAge by remember { mutableStateOf(0) }
    // Self-host node state (drives the label + the inline field). Empty override = the
    // bundled public mainnet node; non-default = the user's own self-hosted beam-node.
    var beamNodeUri by remember { mutableStateOf(HeyApi.beamNode(ctx)) }
    val beamSelfHosted = beamNodeUri.isNotBlank() && beamNodeUri != BeamApi.DEFAULT_NODE
    // Sync mode: "quicksync" (public node, default) / "mobilenode" (private on-device node, loopback)
    // / "ownnode" (FlyClient against a node you host elsewhere).
    var mode by remember { mutableStateOf(HeyApi.beamNodeMode(ctx)) }
    // Quick sync only: the wallet scans (FlyClient) against an official public BEAM node, on a
    // process-scoped coroutine that survives closing this sheet / backgrounding (the foreground
    // RuntimeService keeps the process alive). No on-device node, no loopback.
    // Non-blocking hint (mobile node still reaching peers) — advisory only; the node keeps running.
    var nodeHint by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        bal = withContext(Dispatchers.IO) { runCatching { HeyApi.beamBalance(ctx) }.getOrNull() }
        var lastSynced = false
        while (true) {
            val p = withContext(Dispatchers.IO) { HeyApi.beamSyncProgress() }
            syncing = p.active
            progress = if (p.total > 0L) p else null   // block-height bar (mobile node feeds these atomics)
            val h = if (p.height > 0L) " · block %,d".format(p.height) else ""
            val err = HeyApi.beamSyncError
            // B3: mobile-node staged status ("Starting node…" → "Connecting to peers…" → "Syncing N%"
            // → "Synced"). The stage is set by the watchdog and reflects the node keeping running.
            val stage = HeyApi.beamNodeStage
            nodeHint = HeyApi.beamNodeHint
            status = when {
                p.synced -> "Synced ✓$h"
                stage != null -> stage + h          // mobile node: staged status, node keeps running
                p.active && linkAge > 120 -> "sync looks stuck — restart Hey to retry"
                p.active -> "Syncing…$h"
                err != null -> err.removePrefix("beam: ")
                else -> if (p.height > 0L) "Last sync$h" else ""
            }
            linkAge = if (p.active) linkAge + 1 else 0
            if (p.synced && !lastSynced) bal = withContext(Dispatchers.IO) { runCatching { HeyApi.beamBalance(ctx) }.getOrNull() }
            lastSynced = p.synced
            kotlinx.coroutines.delay(if (p.active || stage != null) 1000L else 3000L)
        }
    }
    // Kick the background sync — idempotent (the shim's single-reactor guard prevents a 2nd run).
    fun sync() { HeyApi.beamSyncStart(ctx) }
    // Auto-sync-on-open: refresh the BEAM balance whenever this sheet opens (quicksync only —
    // a mobile/own node manages its own sync). Idempotent + off-main via beamSyncStart.
    LaunchedEffect(Unit) { if (mode == "quicksync") sync() }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("BEAM", color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            Text("Private Mimblewimble wallet", color = muted, fontSize = 12.sp)
            Spacer(Modifier.height(14.dp))
            // Sync-mode selector. Default quicksync is the recommended, light path.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ModeOption("Quick sync",
                    "Fastest. Verifies your coins against a public BEAM node. Recommended.",
                    "quicksync", mode) { mode = "quicksync"; HeyApi.setBeamNodeMode(ctx, "quicksync"); HeyApi.beamNodeStop() }
                ModeOption("Mobile node",
                    "Run a private BEAM node on this device — no public node sees your wallet. ~1.5–3 GB, first sync can take a while, more battery/data; Wi-Fi + charger recommended.",
                    "mobilenode", mode) { mode = "mobilenode"; HeyApi.setBeamNodeMode(ctx, "mobilenode") }
                ModeOption("Own node",
                    "Point sync at a BEAM node you host elsewhere (host:port).",
                    "ownnode", mode) { mode = "ownnode"; HeyApi.setBeamNodeMode(ctx, "ownnode"); HeyApi.beamNodeStop() }
            }
            Spacer(Modifier.height(12.dp))
            Row(Modifier.fillMaxWidth().glass(12.dp).padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(if (mode == "mobilenode") Icons.Filled.PhonelinkLock else Icons.Filled.Public, null, tint = goldInk, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        when (mode) {
                            "mobilenode" -> "Mobile node · on-device, loopback-only"
                            "ownnode"    -> "Own node · ${beamNodeUri}"
                            else         -> if (beamSelfHosted) "Quick sync · your node" else "Quick sync · public BEAM node"
                        },
                        color = ink, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                    )
                    Text(status.ifBlank { if (syncing) "Syncing…" else "Tap Sync to update your balance" }, color = muted, fontSize = 11.sp)
                }
                // Spin while quick-sync is active OR the mobile node is in a staged (running) state.
                if (syncing || (status.isNotBlank() && status != "Synced ✓" && HeyApi.beamNodeStage != null))
                    CircularProgressIndicator(color = goldInk, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
            }
            // B3: NON-BLOCKING hint — the mobile node still can't reach peers after the grace period.
            // Advisory only; the node KEEPS RUNNING and keeps retrying. Never auto-stopped.
            nodeHint?.let { hint ->
                Spacer(Modifier.height(8.dp))
                Row(Modifier.fillMaxWidth().glass(10.dp).padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Info, null, tint = muted, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(hint, color = muted, fontSize = 11.sp, modifier = Modifier.weight(1f))
                }
            }
            // "Own node": point FlyClient + send at YOUR mainnet beam-node (host:port). Empty
            // = the public default. Same field as Wallet settings → Blockchain nodes.
            if (mode == "ownnode") {
                Spacer(Modifier.height(10.dp))
                RpcNodeRow(
                    RpcNode("beam", "BEAM node (host:port)", BeamApi.DEFAULT_NODE, if (beamSelfHosted) beamNodeUri else ""),
                ) {
                    HeyApi.setBeamNode(ctx, it)
                    beamNodeUri = if (it.isBlank()) BeamApi.DEFAULT_NODE else it
                    "{\"ok\":true}" // BEAM uses a raw TCP host:port, not an http(s) RPC — no H6 gate
                }
            }
            // Live self-host sync bar: real block height + % straight from the on-device node.
            progress?.let { p ->
                Spacer(Modifier.height(10.dp))
                Box(Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(muted.copy(alpha = 0.25f))) {
                    Box(Modifier.fillMaxWidth(if (p.total > 0L) (p.percent / 100f).coerceIn(0.02f, 1f) else 0.04f)
                        .fillMaxHeight().clip(RoundedCornerShape(3.dp)).background(Gold))
                }
                Spacer(Modifier.height(4.dp))
                Text(if (p.total > 0L) "Block ${p.done} / ${p.total} · ${p.percent}%" else "Finding peers…", color = muted, fontSize = 10.sp)
            }
            Spacer(Modifier.height(14.dp))
            BeamAssetRow("BEAM", bal?.beam ?: "—", bal?.beamMaturing, null)
            Spacer(Modifier.height(8.dp))
            BeamAssetRow("BEAMX", bal?.beamx ?: "—", null, "confidential asset #7")
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = onSend, colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy), modifier = Modifier.weight(1f)) {
                    Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Send", fontWeight = FontWeight.Bold)
                }
                OutlinedButton(onClick = onReceive, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Filled.QrCode2, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Receive", color = ink)
                }
            }
            Spacer(Modifier.height(8.dp))
            OutlinedButton(onClick = { sync() }, enabled = !syncing, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Filled.Refresh, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text(if (syncing) "Syncing…" else "Sync balance", color = ink)
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

@Composable
private fun BeamAssetRow(name: String, balance: String, maturing: String?, sub: String?) {
    Row(Modifier.fillMaxWidth().glass(12.dp).padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(name, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
            sub?.let { Text(it, color = muted, fontSize = 11.sp) }
            if (maturing != null && maturing != "0") Text("+$maturing maturing", color = muted, fontSize = 11.sp)
        }
        Text(balance, color = goldInk, fontSize = 18.sp, fontWeight = FontWeight.Bold)
    }
}

/** The "New chat"-style transfer module: recipient + amount → biometric confirm
 *  → sign + broadcast a real ESC value transfer. Money-critical, hence the
 *  explicit review step and small-amount nudge. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SendSheet(chain: String, symbol: String, network: String, token: TokenBal? = null, onClose: () -> Unit, onSent: () -> Unit) {
    val sym = token?.symbol ?: symbol
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val activity = ctx as? androidx.fragment.app.FragmentActivity
    var to by remember { mutableStateOf("") }
    var amount by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf(false) }
    var txHash by remember { mutableStateOf<String?>(null) }
    // Max-fee binding: for a NATIVE EVM send we estimate the fee, show it on the
    // confirm, and bind it (wei) into the spend grant so an inflated RPC gasPrice
    // can't drain extra. ERC-20 fee binding is deferred (the grant stays fee-free).
    var maxFeeWei by remember { mutableStateOf("") }
    var maxFeeDisplay by remember { mutableStateOf("") }
    val scanner = rememberLauncherForActivityResult(ScanContract()) { r ->
        r.contents?.let { to = it.trim().removePrefix("ethereum:").substringBefore("?").substringBefore("@") }
    }

    fun doSend() {
        busy = true; status = "Authorizing…"
        scope.launch {
            // Spend grant (hardware-confirmed when enrolled, else legacy mint),
            // bound to the SAME canonical (kind,to,amount) the signer redeems. The
            // native EVM grant also binds the max fee (maxFeeWei) shown on confirm.
            val grant = if (token != null)
                SpendAuth.spendGrant(activity, "erc20:$chain:${token.contract}", to.trim(), HeyApi.toUnitsHex(amount, token.decimals) ?: "") {
                    HeyApi.authorizeTokenSend(chain, token.contract, to, amount, token.decimals) }
            else
                SpendAuth.spendGrant(activity, "evm:$chain", to.trim(), HeyApi.toWeiHex(amount) ?: "", maxFeeWei) {
                    HeyApi.authorizeEvmSendFee(chain, to, amount, maxFeeWei) }
            if (grant == null) { busy = false; status = "Authorization cancelled"; return@launch }
            status = "Signing & broadcasting…"
            val res = withContext(Dispatchers.IO) {
                if (token != null) HeyApi.tokenSend(ctx, chain, token.contract, to, amount, token.decimals, grant)
                else HeyApi.walletSend(ctx, chain, to, amount, grant)
            }
            busy = false
            res.onSuccess { txHash = it; status = ""; HeyApi.recordTx(ctx, chain, sym, to, amount, it) }
                .onFailure { status = it.message ?: "Send failed" }
        }
    }
    fun review() {
        val amt = amount.trim().toDoubleOrNull()
        val units = if (token != null) HeyApi.toUnitsHex(amount, token.decimals) else HeyApi.toWeiHex(amount)
        if (amt == null || amt <= 0.0 || units == null) { status = "Enter an amount in $sym"; return }
        // Validate + checksum the address (rejects typos / the burn address) and
        // normalize it before the confirm step shows.
        busy = true; status = "Checking address…"
        scope.launch {
            val res = withContext(Dispatchers.IO) { HeyApi.checkAddress(to) }
            if (res.isFailure) { busy = false; status = res.exceptionOrNull()?.message ?: "Invalid recipient address"; return@launch }
            to = res.getOrThrow()
            // Estimate the max fee for a NATIVE EVM send so the confirm can show it
            // and we can bind it into the grant. Best-effort: if it fails we proceed
            // fee-unbound (the per-chain gasPrice ceiling still caps the drain).
            if (token == null) {
                // Estimate against the REAL recipient so a contract `to` (which costs
                // more than 21000 gas) doesn't fail the send closed via the bound (M-1).
                val fee = withContext(Dispatchers.IO) { HeyApi.feeEstimate(ctx, chain, to, amount) }
                maxFeeWei = fee?.optString("maxFeeWei").orEmpty()
                maxFeeDisplay = fee?.let { "${it.optString("maxFee")} ${it.optString("symbol")}" }.orEmpty()
            } else { maxFeeWei = ""; maxFeeDisplay = "" }
            busy = false; status = ""; confirm = true
        }
    }

    ModalBottomSheet(onDismissRequest = { if (!busy) onClose() }, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            if (txHash != null) {
                // A returned hash only means the node accepted the tx — poll the
                // receipt so we report real confirmation, not just broadcast (audit #6).
                var confState by remember { mutableStateOf("pending") }
                LaunchedEffect(txHash) {
                    repeat(24) {
                        kotlinx.coroutines.delay(3000)
                        val s = withContext(Dispatchers.IO) { HeyApi.txStatus(chain, txHash!!) }
                        if (s == "success" || s == "failed") { confState = s; return@LaunchedEffect }
                    }
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    when (confState) {
                        "success" -> Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(56.dp))
                        "failed" -> Icon(Icons.Filled.Error, null, tint = Like, modifier = Modifier.size(56.dp))
                        else -> CircularProgressIndicator(color = goldInk, modifier = Modifier.size(48.dp), strokeWidth = 3.dp)
                    }
                    Spacer(Modifier.height(12.dp))
                    Text(
                        when (confState) { "success" -> "Confirmed"; "failed" -> "Failed on-chain"; else -> "Broadcast" },
                        color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold
                    )
                    Spacer(Modifier.height(6.dp))
                    Text(
                        when (confState) {
                            "success" -> "Your transfer is confirmed on-chain."
                            "failed" -> "The transaction reverted on-chain — gas was spent but the funds were NOT sent. Re-check the recipient and try again."
                            else -> "Sent to the network — confirming on-chain (usually a few seconds)…"
                        },
                        color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center
                    )
                    Spacer(Modifier.height(12.dp))
                    Row(
                        Modifier.clip(RoundedCornerShape(10.dp)).clickable {
                            clipboard.setText(AnnotatedString(txHash!!))
                            android.widget.Toast.makeText(ctx, "Transaction hash copied", android.widget.Toast.LENGTH_SHORT).show()
                        }.padding(8.dp, 4.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text("tx ${shortAddr(txHash!!)}", color = goldInk, fontSize = 12.sp, fontFamily = mono)
                        Spacer(Modifier.width(6.dp)); Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(13.dp))
                    }
                    Spacer(Modifier.height(20.dp))
                    Button(onClick = onSent, modifier = Modifier.fillMaxWidth(), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) { Text("Done", fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.height(16.dp))
                }
            } else {
                Text("Send $sym", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                Text("On $network", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(18.dp))
                OutlinedTextField(
                    value = to, onValueChange = { to = it; status = "" }, singleLine = true,
                    label = { Text("Recipient address (0x…)") },
                    trailingIcon = { IconButton(onClick = { scanner.launch(scanOptions()) }) { Icon(Icons.Filled.QrCodeScanner, "Scan", tint = goldInk) } },
                    textStyle = androidx.compose.ui.text.TextStyle(fontFamily = mono, fontSize = 13.sp),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                Spacer(Modifier.height(12.dp))
                OutlinedTextField(
                    value = amount, onValueChange = { amount = it; status = "" }, singleLine = true,
                    label = { Text("Amount ($sym)") },
                    trailingIcon = { Text(sym, color = muted, fontSize = 13.sp, modifier = Modifier.padding(end = 14.dp)) },
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Decimal),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                Spacer(Modifier.height(14.dp))
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Gold.copy(alpha = 0.10f)).padding(12.dp), verticalAlignment = Alignment.Top) {
                    Icon(Icons.Filled.Info, null, tint = goldInk, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("This sends real $sym and can't be undone. Double-check the address — and send a tiny amount first to be sure.", color = muted, fontSize = 12.sp)
                }
                if (status.isNotBlank()) {
                    Spacer(Modifier.height(10.dp)); Text(status, color = Like, fontSize = 13.sp)
                }
                Spacer(Modifier.height(18.dp))
                Button(onClick = { review() }, enabled = !busy, modifier = Modifier.fillMaxWidth().height(50.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                    else { Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Review & send", fontWeight = FontWeight.Bold) }
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }

    if (confirm) {
        AlertDialog(
            onDismissRequest = { if (!busy) confirm = false },
            icon = { Icon(Icons.AutoMirrored.Filled.Send, null, tint = goldInk) },
            title = { Text("Confirm transfer", color = ink) },
            text = {
                Column {
                    SecureWindow() // block screenshots + tap-jacking on the money confirm
                    Text("$amount $sym", color = ink, fontSize = 26.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(4.dp))
                    Text("to ${shortAddr(to)}", color = muted, fontSize = 13.sp, fontFamily = mono)
                    if (maxFeeDisplay.isNotBlank()) {
                        Spacer(Modifier.height(8.dp))
                        Text("Max network fee: $maxFeeDisplay", color = muted, fontSize = 12.sp)
                    }
                    Spacer(Modifier.height(10.dp))
                    Text("This signs with your key and broadcasts on $network. It cannot be reversed.", color = muted, fontSize = 12.sp)
                }
            },
            confirmButton = {
                TextButton(enabled = !busy, onClick = {
                    spendGate(activity, ctx) { confirm = false; doSend() }
                }) { Text("Sign & send", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { if (!busy) confirm = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

/** Send BEAM or BEAMX (asset 0 / 7 — same wallet & address, like ETH vs an ERC-20). Money-safety:
 *  the first sends are hard-capped sub-cent (BeamApi.SEND_CAP) until the user lifts it after a test,
 *  and every send goes through a Review + (if set) biometric confirm. Broadcasts via a public node. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BeamSendSheet(onClose: () -> Unit, onSent: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val activity = ctx as? androidx.fragment.app.FragmentActivity
    var asset by remember { mutableStateOf(BeamApi.ASSET_BEAM) }
    val sym = if (asset == BeamApi.ASSET_BEAMX) "BEAMX" else "BEAM"
    var to by remember { mutableStateOf("") }
    var amount by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf(false) }
    var txid by remember { mutableStateOf<String?>(null) }
    val capLifted = remember { HeyApi.beamCapLifted(ctx) }
    val scanner = rememberLauncherForActivityResult(ScanContract()) { r -> r.contents?.let { to = it.trim() } }

    fun doSend() {
        busy = true; status = "Authorizing…"
        scope.launch {
            // H1: BEAM now goes through the spend guard like every other chain. Mint a
            // one-shot grant bound to (beam:<asset>, recipient, decimal amount) —
            // hardware-confirmed when enrolled, else the legacy biometric mint.
            val grant = SpendAuth.spendGrant(activity, "beam:$asset", to.trim(), amount.trim()) {
                HeyApi.authorizeBeamSend(asset, to.trim(), amount.trim())
            }
            if (grant == null) { busy = false; status = "Authorization cancelled"; return@launch }
            status = "Building & broadcasting…"
            val res = withContext(Dispatchers.IO) { HeyApi.beamSend(ctx, to.trim(), amount.trim(), asset, grant) }
            busy = false
            res.onSuccess { r -> txid = r.txid; status = ""; HeyApi.recordTx(ctx, "beam", sym, to.trim(), amount.trim(), r.txid) }
                .onFailure { status = it.message ?: "Send failed" }
        }
    }
    fun review() {
        val groth = BeamApi.toGroth(amount.trim())
        if (groth == null || groth <= 0L) { status = "Enter an amount in $sym"; return }
        if (!capLifted && groth > BeamApi.SEND_CAP_GROTH) { status = "Safety cap: first sends are limited to ${BeamApi.SEND_CAP_BEAM} BEAM. Lift it in BEAM settings after a test."; return }
        if (to.isBlank()) { status = "Enter a recipient address"; return }
        busy = true; status = "Checking address…"
        scope.launch {
            val ok = withContext(Dispatchers.IO) { HeyApi.beamValidToken(to.trim()) }
            busy = false
            if (ok) { status = ""; confirm = true } else status = "That doesn't look like a valid BEAM address"
        }
    }

    ModalBottomSheet(onDismissRequest = { if (!busy) onClose() }, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            if (txid != null) {
                var confState by remember { mutableStateOf("pending") }
                LaunchedEffect(txid) {
                    repeat(30) {
                        kotlinx.coroutines.delay(4000)
                        val s = withContext(Dispatchers.IO) { HeyApi.beamTxStatus(ctx, txid!!) }
                        if (s == "confirmed" || s == "failed") { confState = s; return@LaunchedEffect }
                    }
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    when (confState) {
                        "confirmed" -> Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(56.dp))
                        "failed" -> Icon(Icons.Filled.Error, null, tint = Like, modifier = Modifier.size(56.dp))
                        else -> CircularProgressIndicator(color = goldInk, modifier = Modifier.size(48.dp), strokeWidth = 3.dp)
                    }
                    Spacer(Modifier.height(12.dp))
                    Text(when (confState) { "confirmed" -> "Confirmed"; "failed" -> "Failed"; else -> "Broadcast" }, color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(6.dp))
                    Text(
                        when (confState) {
                            "confirmed" -> "Your $sym transfer is confirmed."
                            "failed" -> "The transaction failed — your funds were not sent. Check the recipient and try again."
                            else -> "Sent to the network — confirming (Mimblewimble txs take a little longer)…"
                        },
                        color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center
                    )
                    Spacer(Modifier.height(12.dp))
                    Row(
                        Modifier.clip(RoundedCornerShape(10.dp)).clickable {
                            clipboard.setText(AnnotatedString(txid!!)); android.widget.Toast.makeText(ctx, "Transaction id copied", android.widget.Toast.LENGTH_SHORT).show()
                        }.padding(8.dp, 4.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text("tx ${shortAddr(txid!!)}", color = goldInk, fontSize = 12.sp, fontFamily = mono)
                        Spacer(Modifier.width(6.dp)); Icon(Icons.Filled.ContentCopy, "Copy", tint = muted, modifier = Modifier.size(13.dp))
                    }
                    Spacer(Modifier.height(20.dp))
                    Button(onClick = onSent, modifier = Modifier.fillMaxWidth(), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) { Text("Done", fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.height(16.dp))
                }
            } else {
                Text("Send $sym", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                Text("Private · Mimblewimble", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(16.dp))
                // Asset toggle (same address — the asset is chosen here).
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(glassFill).padding(4.dp)) {
                    listOf(BeamApi.ASSET_BEAM to "BEAM", BeamApi.ASSET_BEAMX to "BEAMX").forEach { (a, label) ->
                        val selected = asset == a
                        Box(
                            Modifier.weight(1f).clip(RoundedCornerShape(9.dp))
                                .background(if (selected) Gold else Color.Transparent)
                                .clickable { asset = a; status = "" }.padding(vertical = 9.dp),
                            contentAlignment = Alignment.Center,
                        ) { Text(label, color = if (selected) Navy else ink, fontWeight = FontWeight.SemiBold, fontSize = 13.sp) }
                    }
                }
                Spacer(Modifier.height(14.dp))
                OutlinedTextField(
                    value = to, onValueChange = { to = it; status = "" }, singleLine = true,
                    label = { Text("Recipient BEAM address") },
                    trailingIcon = { IconButton(onClick = { scanner.launch(scanOptions()) }) { Icon(Icons.Filled.QrCodeScanner, "Scan", tint = goldInk) } },
                    textStyle = androidx.compose.ui.text.TextStyle(fontFamily = mono, fontSize = 12.sp),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                Spacer(Modifier.height(12.dp))
                OutlinedTextField(
                    value = amount, onValueChange = { amount = it; status = "" }, singleLine = true,
                    label = { Text("Amount ($sym)") },
                    trailingIcon = { Text(sym, color = muted, fontSize = 13.sp, modifier = Modifier.padding(end = 14.dp)) },
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Decimal),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                Spacer(Modifier.height(14.dp))
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Gold.copy(alpha = 0.10f)).padding(12.dp), verticalAlignment = Alignment.Top) {
                    Icon(Icons.Filled.Info, null, tint = goldInk, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(
                        if (!capLifted) "This sends real $sym and can't be undone. For safety the first sends are capped at ${BeamApi.SEND_CAP_BEAM} BEAM — do a tiny test, then lift the cap in BEAM settings."
                        else "This sends real $sym and can't be undone. Double-check the address.",
                        color = muted, fontSize = 12.sp
                    )
                }
                if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = Like, fontSize = 13.sp) }
                Spacer(Modifier.height(18.dp))
                Button(onClick = { review() }, enabled = !busy, modifier = Modifier.fillMaxWidth().height(50.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                    else { Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Review & send", fontWeight = FontWeight.Bold) }
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }

    if (confirm) {
        AlertDialog(
            onDismissRequest = { if (!busy) confirm = false },
            icon = { Icon(Icons.AutoMirrored.Filled.Send, null, tint = goldInk) },
            title = { Text("Confirm $sym transfer", color = ink) },
            text = {
                Column {
                    SecureWindow() // block screenshots + tap-jacking on the money confirm
                    Text("$amount $sym", color = ink, fontSize = 26.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(4.dp))
                    Text("to ${shortAddr(to)}", color = muted, fontSize = 13.sp, fontFamily = mono)
                    Spacer(Modifier.height(10.dp))
                    Text("Real $sym on BEAM mainnet. It cannot be reversed.", color = muted, fontSize = 12.sp)
                }
            },
            confirmButton = {
                TextButton(enabled = !busy, onClick = {
                    spendGate(activity, ctx) { confirm = false; doSend() }
                }) { Text("Send", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { if (!busy) confirm = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

/** Tip a feed author by IDENTITY — Hey resolves their receive address from their
 *  signed profile, so you just pick chain + amount. No address, ever. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TipSheet(authorDid: String, authorName: String, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val activity = ctx as? androidx.fragment.app.FragmentActivity
    var loading by remember { mutableStateOf(true) }
    var addresses by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    var chains by remember { mutableStateOf<List<ChainInfo>>(emptyList()) }
    var sel by remember { mutableStateOf<ChainInfo?>(null) }
    var amount by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf(false) }
    var txHash by remember { mutableStateOf<String?>(null) }
    var myTokens by remember { mutableStateOf<List<TokenBal>>(emptyList()) }
    var selTok by remember { mutableStateOf<TokenBal?>(null) } // null = native
    var retry by remember { mutableStateOf(0) }

    LaunchedEffect(authorDid, retry) {
        loading = true
        // refreshContact exchanges tip addresses over the DM channel first, so tipping a
        // chat contact resolves even without following them (then falls back to the cache).
        val a = withContext(Dispatchers.IO) { HeyApi.refreshContact(authorDid) }
        val my = withContext(Dispatchers.IO) { HeyApi.walletChains() }
        addresses = a
        // Tipping is main-chain ELA, ESC and BEAM only — EID is identity
        // plumbing (not money), and the long-tail EVM chains live in the wallet.
        val tippable = ArrayList<ChainInfo>()
        if (a.containsKey("ela")) tippable.add(ChainInfo("ela", "ELA main chain", 0, "ELA"))
        my.firstOrNull { it.key == "esc" && a.containsKey("esc") }?.let { tippable.add(it) }
        if (a.containsKey("beam") && BeamApi.available) tippable.add(ChainInfo("beam", "BEAM private", 0, "BEAM"))
        chains = tippable
        sel = chains.firstOrNull()
        loading = false
    }
    // ERC-20 picker only applies on the EVM chain (ESC).
    LaunchedEffect(sel?.key) {
        selTok = null
        myTokens = if (sel?.key == "esc")
            withContext(Dispatchers.IO) { runCatching { HeyApi.balances(ctx, "esc") }.getOrDefault(emptyList()) }
        else emptyList()
    }
    val tipSym = selTok?.symbol ?: sel?.symbol ?: ""
    fun doSend() {
        val c = sel ?: return; val to = addresses[c.key] ?: return
        busy = true; status = "Authorizing…"
        scope.launch {
            val t = selTok
            // Spend grant (hardware-confirmed when enrolled). H1: BEAM is now under
            // the guard too — mint a "beam:0" grant like every other chain.
            val grant: String? = when {
                c.key == "ela" -> SpendAuth.spendGrant(activity, "ela", to.trim(), amount.trim()) { HeyApi.authorizeElaSend(to, amount) }
                c.key == "beam" -> SpendAuth.spendGrant(activity, "beam:0", to.trim(), amount.trim()) { HeyApi.authorizeBeamSend(0, to, amount) }
                t != null && !t.native -> SpendAuth.spendGrant(activity, "erc20:${c.key}:${t.contract}", to.trim(), HeyApi.toUnitsHex(amount, t.decimals) ?: "") { HeyApi.authorizeTokenSend(c.key, t.contract, to, amount, t.decimals) }
                else -> SpendAuth.spendGrant(activity, "evm:${c.key}", to.trim(), HeyApi.toWeiHex(amount) ?: "") { HeyApi.authorizeEvmSend(c.key, to, amount) }
            }
            if (grant == null) { busy = false; status = "Authorization cancelled"; return@launch }
            status = "Signing & broadcasting…"
            val res = withContext(Dispatchers.IO) {
                when {
                    c.key == "ela" -> HeyApi.elaSend(ctx, to, amount, grant)
                    c.key == "beam" -> HeyApi.beamSend(ctx, to, amount, 0, grant).map { it.txid }
                    t != null && !t.native -> HeyApi.tokenSend(ctx, c.key, t.contract, to, amount, t.decimals, grant)
                    else -> HeyApi.walletSend(ctx, c.key, to, amount, grant)
                }
            }
            busy = false
            res.onSuccess {
                txHash = it; status = ""
                // M1: store the recipient DID (not the display name) in the sealed history.
                HeyApi.recordTx(ctx, c.key, tipSym, authorDid, amount, it, "tip")
                // Tell the recipient over the carrier so they get a tip notification
                // (even with the app closed). Off-main; harmless if it can't reach them.
                scope.launch(Dispatchers.IO) { HeyApi.notifyTip(authorDid, tipSym, amount, it) }
            }.onFailure { status = it.message ?: "Tip failed" }
        }
    }
    fun review() {
        val c = sel ?: run { status = "Pick a chain"; return }
        val amt = amount.trim().toDoubleOrNull()
        if (amt == null || amt <= 0.0) { status = "Enter an amount"; return }
        if (c.key == "esc" && (selTok?.let { HeyApi.toUnitsHex(amount, it.decimals) } ?: HeyApi.toWeiHex(amount)) == null) {
            status = "Enter an amount"; return
        }
        val to = addresses[c.key]
        if (to.isNullOrBlank()) { status = "They haven't published a ${c.symbol} address"; return }
        // Per-chain sanity on the PUBLISHED address before the confirm (the Rust
        // send re-checks everything again).
        when (c.key) {
            "ela" -> { if (!HeyApi.isElaAddress(to)) { status = "Their published address looks invalid" } else confirm = true }
            "beam" -> { if (to.length < 16) { status = "Their published address looks invalid" } else confirm = true }
            else -> {
                busy = true; status = "Checking address…"
                scope.launch {
                    val ok = withContext(Dispatchers.IO) { HeyApi.checkAddress(to) }.isSuccess
                    busy = false
                    if (ok) { status = ""; confirm = true } else status = "Their published address looks invalid"
                }
            }
        }
    }

    ModalBottomSheet(onDismissRequest = { if (!busy) onClose() }, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            if (txHash != null) {
                var confState by remember { mutableStateOf(if (sel?.key == "esc") "pending" else "success") }
                LaunchedEffect(txHash) {
                    if (sel?.key != "esc") return@LaunchedEffect
                    repeat(24) {
                        kotlinx.coroutines.delay(3000)
                        val s = withContext(Dispatchers.IO) { HeyApi.txStatus("esc", txHash!!) }
                        if (s == "success" || s == "failed") { confState = s; return@LaunchedEffect }
                    }
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    when (confState) {
                        "success" -> Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(56.dp))
                        "failed" -> Icon(Icons.Filled.Error, null, tint = Like, modifier = Modifier.size(56.dp))
                        else -> CircularProgressIndicator(color = goldInk, modifier = Modifier.size(48.dp), strokeWidth = 3.dp)
                    }
                    Spacer(Modifier.height(12.dp))
                    Text(when (confState) { "success" -> "Tipped $authorName 🎉"; "failed" -> "Tip failed on-chain"; else -> "Sending tip…" },
                        color = ink, fontSize = 19.sp, fontWeight = FontWeight.Bold, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                    Spacer(Modifier.height(20.dp))
                    Button(onClick = onClose, modifier = Modifier.fillMaxWidth(), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) { Text("Done", fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.height(16.dp))
                }
            } else {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Paid, null, tint = goldInk, modifier = Modifier.size(22.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Tip $authorName", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                }
                Spacer(Modifier.height(4.dp))
                Text("Sent by identity — Hey finds their address. You never need it.", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(16.dp))
                when {
                    loading -> Box(Modifier.fillMaxWidth().padding(20.dp), Alignment.Center) { CircularProgressIndicator(color = goldInk) }
                    chains.isEmpty() -> {
                        Text("We don't have $authorName's wallet address yet.", color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                        Spacer(Modifier.height(6.dp))
                        Text("There's no server — their address arrives with their profile over the network. If you follow them it usually syncs within moments (they may also need to update Hey). Try again in a bit.",
                            color = muted, fontSize = 13.sp, lineHeight = 19.sp)
                        Spacer(Modifier.height(16.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Button(onClick = { retry++ }, modifier = Modifier.weight(1f), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                                Icon(Icons.Filled.Refresh, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Try again", fontWeight = FontWeight.Bold)
                            }
                            OutlinedButton(onClick = onClose, modifier = Modifier.weight(1f)) { Text("Close", color = ink) }
                        }
                        Spacer(Modifier.height(8.dp))
                    }
                    else -> {
                        Text("Chain", color = muted, fontSize = 12.sp)
                        Spacer(Modifier.height(6.dp))
                        // FlowRow: chips WRAP to the next line — a Row squeezed the
                        // third chip into a 1-character-wide tower (the giant-gap bug)
                        @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
                        androidx.compose.foundation.layout.FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            chains.forEach { c ->
                                val on = sel?.key == c.key
                                val label = when (c.key) {
                                    "ela" -> "ELA · main chain"
                                    "esc" -> "ESC"
                                    "beam" -> "BEAM"
                                    else -> c.symbol
                                }
                                Box(
                                    Modifier.clip(RoundedCornerShape(20.dp))
                                        .background(if (on) Gold.copy(alpha = 0.22f) else glassFill)
                                        .border(1.dp, if (on) goldInk else glassBorder, RoundedCornerShape(20.dp))
                                        .clickable { sel = c }.padding(14.dp, 8.dp)
                                ) { Text(label, color = if (on) goldInk else ink, fontSize = 13.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal) }
                            }
                        }
                        // Asset picker: native or any ERC-20 you hold on this chain.
                        if (sel?.key == "esc" && myTokens.size > 1) {
                            Spacer(Modifier.height(10.dp))
                            Text("Asset", color = muted, fontSize = 12.sp)
                            Spacer(Modifier.height(6.dp))
                            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                myTokens.forEach { t ->
                                    val on = if (t.native) selTok == null else selTok?.contract == t.contract
                                    Box(
                                        Modifier.clip(RoundedCornerShape(20.dp))
                                            .background(if (on) Gold.copy(alpha = 0.22f) else glassFill)
                                            .border(1.dp, if (on) goldInk else glassBorder, RoundedCornerShape(20.dp))
                                            .clickable { selTok = if (t.native) null else t }.padding(14.dp, 8.dp)
                                    ) { Text(t.symbol, color = if (on) goldInk else ink, fontSize = 13.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal) }
                                }
                            }
                        }
                        Spacer(Modifier.height(12.dp))
                        OutlinedTextField(
                            value = amount, onValueChange = { amount = it; status = "" }, singleLine = true,
                            label = { Text("Amount ($tipSym)") },
                            keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Decimal),
                            modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                        )
                        if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = Like, fontSize = 13.sp) }
                        Spacer(Modifier.height(16.dp))
                        Button(onClick = { review() }, enabled = !busy, modifier = Modifier.fillMaxWidth().height(50.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                            if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                            else { Icon(Icons.Filled.Paid, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Review & tip", fontWeight = FontWeight.Bold) }
                        }
                        Spacer(Modifier.height(14.dp))
                    }
                }
            }
        }
    }

    if (confirm && sel != null) {
        val c = sel!!
        AlertDialog(
            onDismissRequest = { if (!busy) confirm = false },
            icon = { Icon(Icons.Filled.Paid, null, tint = goldInk) },
            title = { Text("Confirm tip", color = ink) },
            text = {
                Column {
                    SecureWindow() // block screenshots + tap-jacking on the money confirm
                    Text("$amount $tipSym", color = ink, fontSize = 26.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(4.dp))
                    Text("to $authorName · on ${c.name}", color = muted, fontSize = 13.sp)
                    Spacer(Modifier.height(10.dp))
                    Text("Signs with your key and broadcasts on-chain. It cannot be reversed.", color = muted, fontSize = 12.sp)
                }
            },
            confirmButton = {
                TextButton(enabled = !busy, onClick = {
                    spendGate(activity, ctx) { confirm = false; doSend() }
                }) { Text("Sign & tip", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { if (!busy) confirm = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

/** Send native ELA on the Elastos MAINCHAIN (UTXO). Recipient is an 'E…' address;
 *  the full address validation + signing happen in Rust (byte-exact, P-256). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ElaSendSheet(onClose: () -> Unit, onSent: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val activity = ctx as? androidx.fragment.app.FragmentActivity
    var to by remember { mutableStateOf("") }
    var amount by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf(false) }
    var txHash by remember { mutableStateOf<String?>(null) }
    val scanner = rememberLauncherForActivityResult(ScanContract()) { r ->
        r.contents?.let { to = it.trim().removePrefix("elastos:").substringBefore("?") }
    }

    fun doSend() {
        busy = true; status = "Authorizing…"
        scope.launch {
            // Spend grant: hardware-confirmed (per-op biometric) when enrolled,
            // else the legacy UI-biometric mint. null = the user cancelled → abort.
            val grant = SpendAuth.spendGrant(activity, "ela", to.trim(), amount.trim()) { HeyApi.authorizeElaSend(to, amount) }
            if (grant == null) { busy = false; status = "Authorization cancelled"; return@launch }
            status = "Signing & broadcasting…"
            val res = withContext(Dispatchers.IO) { HeyApi.elaSend(ctx, to, amount, grant) }
            busy = false
            res.onSuccess { txHash = it; status = ""; HeyApi.recordTx(ctx, "ela", "ELA", to, amount, it) }
                .onFailure { status = it.message ?: "Send failed" }
        }
    }
    fun review() {
        if (!HeyApi.isElaAddress(to)) { status = "Enter a valid Elastos mainchain address (starts with E)"; return }
        val amt = amount.trim().toDoubleOrNull()
        if (amt == null || amt <= 0.0) { status = "Enter an amount in ELA"; return }
        status = ""; confirm = true
    }

    ModalBottomSheet(onDismissRequest = { if (!busy) onClose() }, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            if (txHash != null) {
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(Icons.Filled.CheckCircle, null, tint = good, modifier = Modifier.size(56.dp))
                    Spacer(Modifier.height(12.dp))
                    Text("Broadcast", color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(6.dp))
                    Text("Your ELA transfer is on the mainchain — it confirms in a couple of minutes.", color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                    Spacer(Modifier.height(12.dp))
                    Text("tx ${shortAddr(txHash!!)}", color = goldInk, fontSize = 12.sp, fontFamily = mono)
                    Spacer(Modifier.height(20.dp))
                    Button(onClick = onSent, modifier = Modifier.fillMaxWidth(), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) { Text("Done", fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.height(16.dp))
                }
            } else {
                Text("Send ELA", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                Text("On the Elastos Mainchain", color = muted, fontSize = 12.sp)
                Spacer(Modifier.height(18.dp))
                OutlinedTextField(
                    value = to, onValueChange = { to = it; status = "" }, singleLine = true,
                    label = { Text("Recipient address (E…)") },
                    trailingIcon = { IconButton(onClick = { scanner.launch(scanOptions()) }) { Icon(Icons.Filled.QrCodeScanner, "Scan", tint = goldInk) } },
                    textStyle = androidx.compose.ui.text.TextStyle(fontFamily = mono, fontSize = 13.sp),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                Spacer(Modifier.height(12.dp))
                OutlinedTextField(
                    value = amount, onValueChange = { amount = it; status = "" }, singleLine = true,
                    label = { Text("Amount (ELA)") },
                    trailingIcon = { Text("ELA", color = muted, fontSize = 13.sp, modifier = Modifier.padding(end = 14.dp)) },
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Decimal),
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors()
                )
                Spacer(Modifier.height(14.dp))
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Gold.copy(alpha = 0.10f)).padding(12.dp), verticalAlignment = Alignment.Top) {
                    Icon(Icons.Filled.Info, null, tint = goldInk, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("This sends real ELA on the mainchain and can't be undone. Double-check the address — send a tiny amount first.", color = muted, fontSize = 12.sp)
                }
                if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = Like, fontSize = 13.sp) }
                Spacer(Modifier.height(18.dp))
                Button(onClick = { review() }, enabled = !busy, modifier = Modifier.fillMaxWidth().height(50.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    if (busy) CircularProgressIndicator(color = Navy, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                    else { Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Review & send", fontWeight = FontWeight.Bold) }
                }
                Spacer(Modifier.height(16.dp))
            }
        }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { if (!busy) confirm = false },
            icon = { Icon(Icons.AutoMirrored.Filled.Send, null, tint = goldInk) },
            title = { Text("Confirm transfer", color = ink) },
            text = {
                Column {
                    SecureWindow() // block screenshots + tap-jacking on the money confirm
                    Text("$amount ELA", color = ink, fontSize = 26.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(4.dp))
                    Text("to ${shortAddr(to)} · Elastos Mainchain", color = muted, fontSize = 13.sp, fontFamily = mono)
                    Spacer(Modifier.height(10.dp))
                    Text("Signs with your key and broadcasts on the mainchain. It cannot be reversed.", color = muted, fontSize = 12.sp)
                }
            },
            confirmButton = {
                TextButton(enabled = !busy, onClick = {
                    spendGate(activity, ctx) { confirm = false; doSend() }
                }) { Text("Sign & send", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { if (!busy) confirm = false }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

private fun scanOptions() = com.journeyapps.barcodescanner.ScanOptions().apply {
    setDesiredBarcodeFormats(com.journeyapps.barcodescanner.ScanOptions.QR_CODE)
    setOrientationLocked(false); setBeepEnabled(false); setPrompt("Scan a wallet address")
    setCaptureActivity(PortraitCaptureActivity::class.java)
}

// ── chat (conversation list → message-bubble thread → composer) ──────────────

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
fun ChatListScreen(topPad: Dp = 12.dp, onOpen: (Chat) -> Unit) {
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    var chats by remember { mutableStateOf<List<Chat>>(emptyList()) }
    var loaded by remember { mutableStateOf(false) }
    var showAdd by remember { mutableStateOf(false) }
    var showNewGroup by remember { mutableStateOf(false) }
    var toDelete by remember { mutableStateOf<Chat?>(null) }
    var reloadTick by remember { mutableStateOf(0) }
    LaunchedEffect(reloadTick) {
        while (true) {
            // Hide blocked contacts (Block & remove in the chat-info modal).
            val blocked = HeyApi.blockedDids(ctx)
            chats = withContext(Dispatchers.IO) { runCatching { HeyApi.chats() }.getOrDefault(emptyList()) }
                .filter { it.isGroup || it.id !in blocked }
            loaded = true
            kotlinx.coroutines.delay(2000)
        }
    }
    Box(Modifier.fillMaxSize()) {
        if (loaded && chats.isEmpty()) {
            Column(Modifier.fillMaxSize(), Arrangement.Center, Alignment.CenterHorizontally) {
                Icon(Icons.Filled.Forum, null, tint = muted, modifier = Modifier.size(48.dp))
                Spacer(Modifier.height(12.dp))
                Text("No conversations yet", color = ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                Text("Tap + to message someone you follow, or paste a friend link.", color = muted, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            }
        } else {
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = topPad, bottom = 96.dp)) {
                items(chats, key = { (if (it.isGroup) "g:" else "d:") + it.id }) {
                    ChatRow(it, onClick = { onOpen(it) }, onLongClick = { toDelete = it })
                }
            }
        }
        Column(Modifier.align(Alignment.BottomEnd).padding(end = 20.dp, bottom = 96.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            SmallFloatingActionButton(
                onClick = { showNewGroup = true }, containerColor = sheetBg, contentColor = goldInk,
            ) { Icon(Icons.Filled.GroupAdd, "New group") }
            Spacer(Modifier.height(12.dp))
            FloatingActionButton(
                onClick = { showAdd = true }, containerColor = Gold, contentColor = Navy,
            ) { Icon(Icons.Filled.PersonAdd, "Add contact") }
        }
    }
    if (showAdd) AddContactSheet(
        onClose = { showAdd = false },
        onStartChat = { did -> showAdd = false; onOpen(Chat(did, HeyApi.shortDid(did), "", 0, 0, false)) },
    )
    if (showNewGroup) NewGroupSheet(onClose = { showNewGroup = false }, onCreated = { reloadTick++ })
    toDelete?.let { c ->
        AlertDialog(
            onDismissRequest = { toDelete = null },
            title = { Text(if (c.isGroup) "Leave & delete group?" else "Delete conversation?", color = ink) },
            text = { Text(c.name, color = muted) },
            confirmButton = {
                TextButton(onClick = {
                    toDelete = null
                    scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.deleteChat(c) } }; reloadTick++ }
                }) { Text("Delete", color = Like) }
            },
            dismissButton = { TextButton(onClick = { toDelete = null }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
private fun ChatRow(chat: Chat, onClick: () -> Unit, onLongClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 5.dp).glass(14.dp)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick).padding(12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (chat.isGroup)
            Box(Modifier.size(46.dp).clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                Icon(Icons.Filled.Group, null, tint = Navy)
            }
        else Avatar(chat.avatar, chat.id, 46)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(chat.name, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(chat.preview.ifBlank { "Tap to chat" }, color = muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(horizontalAlignment = Alignment.End) {
            if (chat.ts > 0) Text(relativeTime(chat.ts), color = muted, fontSize = 11.sp)
            if (chat.unread > 0) {
                Spacer(Modifier.height(4.dp))
                Box(Modifier.clip(RoundedCornerShape(11.dp)).background(Like).padding(7.dp, 2.dp), Alignment.Center) {
                    Text("${chat.unread}", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

/** A file/photo the user picked but hasn't sent yet (staged in the composer tray). */
private data class StagedItem(val uri: Uri, val mime: String, val isImage: Boolean)

/** Soft top/bottom alpha fade so messages dissolve (rather than hard-cut) as they scroll under the
 *  floating glass bars. Uses an offscreen layer + a DstIn gradient mask. */
// Smooth-step alpha stops (near-zero slope at both ends → no visible "line" where the fade meets
// full-opacity content). Used as a DstIn mask: Black=keep, Transparent=fade out.
private val fadeMaskTop = arrayOf(
    0.00f to Color.Transparent,
    0.12f to Color.Black.copy(alpha = 0.03f),
    0.28f to Color.Black.copy(alpha = 0.15f),
    0.45f to Color.Black.copy(alpha = 0.38f),
    0.60f to Color.Black.copy(alpha = 0.62f),
    0.75f to Color.Black.copy(alpha = 0.83f),
    0.88f to Color.Black.copy(alpha = 0.95f),
    1.00f to Color.Black,
)
private val fadeMaskBottom = arrayOf(
    0.00f to Color.Black,
    0.12f to Color.Black.copy(alpha = 0.95f),
    0.25f to Color.Black.copy(alpha = 0.83f),
    0.40f to Color.Black.copy(alpha = 0.62f),
    0.55f to Color.Black.copy(alpha = 0.38f),
    0.72f to Color.Black.copy(alpha = 0.15f),
    0.88f to Color.Black.copy(alpha = 0.03f),
    1.00f to Color.Transparent,
)

private fun Modifier.fadeEdges(top: Dp, bottom: Dp): Modifier = this
    .graphicsLayer { compositingStrategy = androidx.compose.ui.graphics.CompositingStrategy.Offscreen }
    .drawWithContent {
        drawContent()
        if (top.toPx() > 0f) drawRect(
            brush = Brush.verticalGradient(*fadeMaskTop, startY = 0f, endY = top.toPx()),
            blendMode = androidx.compose.ui.graphics.BlendMode.DstIn,
        )
        if (bottom.toPx() > 0f) drawRect(
            brush = Brush.verticalGradient(*fadeMaskBottom, startY = size.height - bottom.toPx(), endY = size.height),
            blendMode = androidx.compose.ui.graphics.BlendMode.DstIn,
        )
    }

/** Chat-info modal (tap the contact in the conversation header): view profile, call, tip/gift,
 *  mute, block & remove, and a grid of all photos shared in the chat. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatInfoSheet(
    chat: Chat,
    msgs: List<Msg>,
    onViewProfile: () -> Unit,
    onCall: () -> Unit,
    onTip: () -> Unit,
    onBlocked: () -> Unit,
    onClose: () -> Unit,
) {
    val ctx = LocalContext.current
    var isMuted by remember { mutableStateOf(HeyApi.isChatMuted(ctx, chat.id)) }
    var viewer by remember { mutableStateOf<Attachment?>(null) }
    val photos = remember(msgs.size) { msgs.flatMap { it.attachments }.filter { it.isImage } }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Avatar(chat.avatar, chat.id, 56)
                Spacer(Modifier.width(14.dp))
                Column {
                    Text(chat.name, color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Lock, null, tint = good, modifier = Modifier.size(11.dp))
                        Spacer(Modifier.width(3.dp))
                        Text("end-to-end encrypted", color = muted, fontSize = 12.sp)
                    }
                    // Live transport to this contact (1:1 only): direct P2P vs relay.
                    if (!chat.isGroup) {
                        var transport by remember(chat.id) { mutableStateOf("") }
                        LaunchedEffect(chat.id) {
                            transport = withContext(Dispatchers.IO) { HeyApi.contactTransport(chat.id) }
                        }
                        if (transport.isNotEmpty()) {
                            val (dot, label) = when (transport) {
                                "direct" -> good to "Direct P2P"
                                "relay" -> goldInk to "Via relay"
                                else -> muted to "Offline"
                            }
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text("●", color = dot, fontSize = 11.sp)
                                Spacer(Modifier.width(3.dp))
                                Text(label, color = muted, fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
            ChatInfoAction(Icons.Filled.Person, "View profile") { onClose(); onViewProfile() }
            ChatInfoAction(Icons.Filled.Call, "Voice call") { onClose(); onCall() }
            ChatInfoAction(Icons.Filled.Paid, "Send a gift / tip") { onClose(); onTip() }
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                    .clickable { isMuted = !isMuted; HeyApi.setChatMuted(ctx, chat.id, isMuted) }.padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(if (isMuted) Icons.Filled.NotificationsOff else Icons.Filled.Notifications, null, tint = goldInk)
                Spacer(Modifier.width(14.dp))
                Text("Mute notifications", color = ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                Switch(checked = isMuted, onCheckedChange = { isMuted = it; HeyApi.setChatMuted(ctx, chat.id, it) })
            }
            ChatInfoAction(Icons.Filled.Block, "Block & remove", danger = true) {
                HeyApi.setBlocked(ctx, chat.id, true); HeyApi.deleteChat(chat); onClose(); onBlocked()
            }
            if (photos.isNotEmpty()) {
                Spacer(Modifier.height(18.dp))
                Text("Shared photos · ${photos.size}", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                Spacer(Modifier.height(10.dp))
                photos.chunked(3).forEach { row ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        row.forEach { att -> SharedPhoto(att, Modifier.weight(1f)) { viewer = att } }
                        repeat(3 - row.size) { Spacer(Modifier.weight(1f)) }
                    }
                    Spacer(Modifier.height(6.dp))
                }
            }
            Spacer(Modifier.height(20.dp))
        }
    }
    viewer?.let { att ->
        var bytes by remember(att.raw) { mutableStateOf<ByteArray?>(null) }
        LaunchedEffect(att.raw) {
            bytes = withContext(Dispatchers.IO) { runCatching { HeyApi.fetchAttachment(att) }.getOrNull()?.takeIf { it.isNotEmpty() } }
        }
        bytes?.let { FullImageViewer(it, att.name) { viewer = null } }
    }
}

@Composable
private fun ChatInfoAction(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, danger: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable { onClick() }.padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = if (danger) Like else goldInk)
        Spacer(Modifier.width(14.dp))
        Text(label, color = if (danger) Like else ink, fontSize = 15.sp)
    }
}

@Composable
private fun SharedPhoto(att: Attachment, modifier: Modifier, onOpen: () -> Unit) {
    var bmp by remember(att.raw) { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(att.raw) {
        val raw = withContext(Dispatchers.IO) { runCatching { HeyApi.fetchAttachment(att) }.getOrNull()?.takeIf { it.isNotEmpty() } }
        bmp = raw?.let { withContext(Dispatchers.IO) { runCatching { BitmapFactory.decodeByteArray(it, 0, it.size) }.getOrNull() } }
    }
    Box(modifier.aspectRatio(1f).clip(RoundedCornerShape(8.dp)).background(glassFill).clickable { onOpen() }, Alignment.Center) {
        bmp?.let { Image(it.asImageBitmap(), att.name, Modifier.matchParentSize(), contentScale = ContentScale.Crop) }
            ?: CircularProgressIndicator(Modifier.size(18.dp), color = goldInk, strokeWidth = 2.dp)
    }
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
fun ConversationScreen(chat: Chat, onBack: () -> Unit) {
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    var msgs by remember { mutableStateOf<List<Msg>>(emptyList()) }
    var reactions by remember { mutableStateOf<Map<String, List<MsgReaction>>>(emptyMap()) }
    var input by remember { mutableStateOf("") }
    var query by remember { mutableStateOf<String?>(null) }   // null = search closed
    var sending by remember { mutableStateOf(false) }
    var transferLabel by remember { mutableStateOf<String?>(null) }
    var staged by remember { mutableStateOf<List<StagedItem>>(emptyList()) }
    var reactTarget by remember { mutableStateOf<String?>(null) }
    var deleteTarget by remember { mutableStateOf<String?>(null) }
    var actionTarget by remember { mutableStateOf<Msg?>(null) }   // long-press on OWN message
    var editTarget by remember { mutableStateOf<Msg?>(null) }
    var showCryptoSend by remember { mutableStateOf(false) }
    var showChatInfo by remember { mutableStateOf(false) }
    var profileDid by remember { mutableStateOf<String?>(null) }
    // Group call: poll the thread for a joinable call so we can show a "Join" banner + route the header button.
    var activeCall by remember { mutableStateOf<JSONObject?>(null) }
    if (chat.isGroup) LaunchedEffect(chat.id) {
        while (true) {
            activeCall = withContext(Dispatchers.IO) {
                runCatching { HeyApi.groupActiveCall(chat.id).takeIf { it.optBoolean("active") } }.getOrNull()
            }
            kotlinx.coroutines.delay(2500)
        }
    }
    fun groupCallTap() {
        val cid = activeCall?.optString("call_id").orEmpty()
        if (cid.isNotEmpty()) CallManager.joinGroupCall(chat.id, cid, chat.name)
        else CallManager.startGroupCall(chat.id, chat.name)
    }
    val listState = rememberLazyListState()
    suspend fun reload() {
        val m = withContext(Dispatchers.IO) { runCatching { HeyApi.conversation(chat) }.getOrDefault(msgs) }
        if (m != msgs) msgs = m
        val r = withContext(Dispatchers.IO) { runCatching { HeyApi.messageReactions(chat) }.getOrDefault(reactions) }
        if (r != reactions) reactions = r
    }
    // Multi-select: STAGE the picked files/photos (no auto-send). The user reviews them in the tray
    // below, optionally adds a caption, then taps Send. Images are compressed to sharp WebP at send.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        val add = uris.map { u ->
            val mime = ctx.contentResolver.getType(u) ?: "application/octet-stream"
            StagedItem(u, mime, mime.startsWith("image/") && mime != "image/gif")
        }
        staged = (staged + add).take(10) // cap a batch at 10
    }
    // Send the staged attachments (+ optional caption), then clear the tray. Runs off the main thread.
    fun sendStaged() {
        if (sending) return
        val text = input.trim()
        val items = staged
        if (text.isEmpty() && items.isEmpty()) return
        input = ""; staged = emptyList()
        sending = true
        transferLabel = if (items.isEmpty()) null
            else "Sending ${items.size} ${if (items.size == 1) "item" else "items"}…"
        scope.launch {
            KeepAwake.on(ctx) // keep the screen awake while the upload is in flight
            withContext(Dispatchers.IO) {
                if (text.isNotEmpty()) runCatching { HeyApi.send(chat, text) }
                for (it in items) {
                    runCatching {
                        // IMAGES: scaled WebP over the bytes path (small).
                        if (it.isImage) {
                            val (bytes, mime, name) = readUri(ctx, it.uri) ?: return@runCatching
                            HeyApi.sendAttachment(chat, scaleWebp(bytes, 2048, 85), "image/webp", name.substringBeforeLast('.', name) + ".webp")
                            return@runCatching
                        }
                        val sz = uriSize(ctx, it.uri)
                        val relay = HeyApi.contactTransport(chat.id) == "relay"
                        if (relay) {
                            // RELAY: keep the bytes path + a small cap so a big file can't
                            // flood the shared relay.
                            if (sz > RELAY_ATTACH_BYTES) {
                                withContext(Dispatchers.Main) {
                                    android.widget.Toast.makeText(
                                        ctx,
                                        "File too large for a relay link (${sz / 1024 / 1024} MB). Max ${RELAY_ATTACH_BYTES / 1024 / 1024} MB over relay — connect directly (same Wi-Fi) to send more.",
                                        android.widget.Toast.LENGTH_LONG
                                    ).show()
                                }
                                return@runCatching
                            }
                            val (bytes, mime, name) = readUri(ctx, it.uri) ?: return@runCatching
                            HeyApi.sendAttachment(chat, bytes, mime, name)
                        } else {
                            // DIRECT P2P: torrent-style STREAMED send from a temp file — the file
                            // is read/encrypted/uploaded one 256 KB chunk at a time (O(chunk) RAM),
                            // so it's effectively unlimited (no 64 MB ceiling).
                            if (sz > MAX_STREAMED_BYTES) {
                                withContext(Dispatchers.Main) {
                                    android.widget.Toast.makeText(
                                        ctx,
                                        "File too large (${sz / 1024 / 1024 / 1024} GB). Max ${MAX_STREAMED_BYTES / 1024 / 1024 / 1024} GB.",
                                        android.widget.Toast.LENGTH_LONG
                                    ).show()
                                }
                                return@runCatching
                            }
                            val staged = copyUriToTemp(ctx, it.uri) ?: return@runCatching
                            try {
                                HeyApi.sendAttachmentPath(chat, staged.first.absolutePath, staged.second, staged.third)
                            } finally {
                                staged.first.delete()
                            }
                        }
                    }
                }
            }
            KeepAwake.off(ctx)
            transferLabel = null
            sending = false
            reload()
        }
    }
    LaunchedEffect(chat.id) {
        // Mark read every cycle while the chat is open — covers GROUPS (which were
        // never marked read) AND messages that arrive while you're viewing, so the
        // unread badge actually clears and stays clear.
        while (true) {
            withContext(Dispatchers.IO) { HeyApi.markRead(chat) }
            reload()
            kotlinx.coroutines.delay(1500)
        }
    }
    LaunchedEffect(msgs.size) {
        if (msgs.isEmpty()) return@LaunchedEffect
        val last = listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
        if (last >= msgs.size - 3) runCatching { listState.animateScrollToItem(msgs.size - 1) }
    }

    val shown = query?.takeIf { it.isNotBlank() }?.let { q -> msgs.filter { it.text.contains(q, ignoreCase = true) } } ?: msgs

    Box(
        Modifier.fillMaxSize().pointerInput(Unit) {
            // Swipe in from the left edge → go back (iOS-style).
            var startX = 0f; var dx = 0f
            detectHorizontalDragGestures(
                onDragStart = { startX = it.x; dx = 0f },
                onDragEnd = { if (startX < 56.dp.toPx() && dx > 110.dp.toPx()) onBack() },
            ) { _, delta -> dx += delta }
        }
    ) {
        val topInset = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
        val botInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
        // Messages scroll the FULL height, behind the floating glass bars (like the feed dock), and
        // softly fade out at the top/bottom edges where they meet the bars.
        LazyColumn(
            Modifier.fillMaxSize().fadeEdges(topInset + 58.dp, botInset + 70.dp),
            state = listState,
            contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = topInset + 66.dp, bottom = botInset + 78.dp),
        ) {
            items(shown, key = { it.id }) { m ->
                Bubble(
                    m, chat.isGroup, reactions[m.id].orEmpty(),
                    // Hold YOUR message → delete it; hold a RECEIVED message → react to it.
                    onLongPress = { if (m.mine) actionTarget = m else reactTarget = m.id },
                    onReact = { e -> scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.reactToMessage(chat, m.id, e) } }; reload() } },
                )
            }
        }
        // ── Floating glass HEADER ──
        Row(
            Modifier.align(Alignment.TopCenter).fillMaxWidth().statusBarsPadding()
                .padding(horizontal = 10.dp, vertical = 8.dp)
                .clip(RoundedCornerShape(24.dp))
                .background(bg2.copy(alpha = 0.84f))
                .background(Brush.verticalGradient(listOf(Color.White.copy(alpha = if (heyLight) 0.18f else 0.07f), Color.White.copy(alpha = 0.02f))))
                .border(1.dp, glassBorder, RoundedCornerShape(24.dp))
                .padding(horizontal = 4.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = ink) }
            if (query != null) {
                OutlinedTextField(
                    value = query ?: "", onValueChange = { query = it }, singleLine = true,
                    placeholder = { Text("Search messages…", color = muted) },
                    modifier = Modifier.weight(1f), colors = glassFieldColors(),
                    textStyle = androidx.compose.ui.text.TextStyle(color = ink),
                )
                IconButton(onClick = { query = null }) { Icon(Icons.Filled.Close, "Close search", tint = ink) }
            } else {
                if (chat.isGroup)
                    Box(Modifier.size(40.dp).clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                        Icon(Icons.Filled.Group, null, tint = Navy, modifier = Modifier.size(20.dp))
                    }
                else Avatar(chat.avatar, chat.id, 40) { showChatInfo = true }
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f).then(if (!chat.isGroup) Modifier.clickable { showChatInfo = true } else Modifier)) {
                    Text(chat.name, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 17.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Lock, null, tint = good, modifier = Modifier.size(11.dp))
                        Spacer(Modifier.width(3.dp))
                        Text(if (chat.isGroup) "group · end-to-end encrypted" else "end-to-end encrypted", color = muted, fontSize = 11.sp, maxLines = 1)
                    }
                }
                // Voice call: 1:1 rings the contact; group starts/joins a mesh call announced in the thread.
                if (!chat.isGroup) IconButton(onClick = { CallManager.startCall(chat.id, chat.name) }) { Icon(Icons.Filled.Call, "Voice call", tint = goldInk) }
                else IconButton(onClick = { groupCallTap() }) { Icon(Icons.Filled.Call, "Group call", tint = if (activeCall != null) good else goldInk) }
                // Video call (1:1, DIRECT-ONLY): gold + live when the contact is on a direct path,
                // greyed otherwise (a video stream would flood a relay).
                if (!chat.isGroup) {
                    val vctx = LocalContext.current
                    var directLink by remember(chat.id) { mutableStateOf(false) }
                    LaunchedEffect(chat.id) {
                        while (true) {
                            directLink = withContext(Dispatchers.IO) { HeyApi.contactTransport(chat.id) == "direct" }
                            kotlinx.coroutines.delay(3000)
                        }
                    }
                    IconButton(onClick = {
                        if (directLink) CallManager.startCall(chat.id, chat.name, video = true)
                        else android.widget.Toast.makeText(vctx, "Video needs a direct link — get on the same Wi-Fi, or both reachable.", android.widget.Toast.LENGTH_LONG).show()
                    }) { Icon(Icons.Filled.Videocam, "Video call", tint = if (directLink) goldInk else muted) }
                }
                // Send crypto to this contact (DMs only — by identity, no address).
                if (!chat.isGroup) IconButton(onClick = { showCryptoSend = true }) { Icon(Icons.Filled.Paid, "Send crypto", tint = goldInk) }
                IconButton(onClick = { query = "" }) { Icon(Icons.Filled.Search, "Search", tint = muted) }
            }
        }
        // ── Join-call banner: a live group call I'm not in → tap to join (a "message everyone can tap") ──
        val ac = activeCall
        val inThisCall = (CallManager.state as? CallManager.State.GroupActive)?.callId == ac?.optString("call_id")
        if (chat.isGroup && ac != null && !inThisCall) {
            val n = ac.optJSONArray("participants")?.length() ?: 0
            Row(
                Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(top = 64.dp)
                    .clip(RoundedCornerShape(20.dp))
                    .background(good.copy(alpha = 0.16f))
                    .background(Brush.verticalGradient(listOf(Color.White.copy(alpha = if (heyLight) 0.18f else 0.07f), Color.White.copy(alpha = 0.02f))))
                    .border(1.dp, good.copy(alpha = 0.4f), RoundedCornerShape(20.dp))
                    .clickable { groupCallTap() }
                    .padding(horizontal = 14.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Filled.Call, null, tint = good, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(
                    if (n > 0) "Group call · $n in call" else "Group call started",
                    color = ink, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                )
                Spacer(Modifier.width(12.dp))
                Box(Modifier.clip(RoundedCornerShape(14.dp)).background(good).padding(horizontal = 14.dp, vertical = 5.dp)) {
                    Text("Join", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
        // ── Floating glass BOTTOM bar (staged tray + transfer + input) ──
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().navigationBarsPadding()
                .padding(horizontal = 10.dp, vertical = 8.dp),
        ) {
            // Staged attachments tray — review before sending; × removes, the attach button adds more.
            if (staged.isNotEmpty() && !sending) {
                LazyRow(
                    Modifier.fillMaxWidth().padding(vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(staged, key = { it.uri.toString() }) { att ->
                        Box(Modifier.size(64.dp)) {
                            Box(Modifier.matchParentSize().clip(RoundedCornerShape(10.dp)).background(glassFill), Alignment.Center) {
                                if (att.isImage) AsyncImage(
                                    model = att.uri, contentDescription = null,
                                    contentScale = ContentScale.Crop,
                                    modifier = Modifier.matchParentSize().clip(RoundedCornerShape(10.dp)),
                                ) else Icon(Icons.Filled.InsertDriveFile, null, tint = goldInk, modifier = Modifier.size(28.dp))
                            }
                            Box(
                                Modifier.align(Alignment.TopEnd).padding(2.dp).size(20.dp).clip(CircleShape)
                                    .background(Color(0xCC000000)).clickable { staged = staged - att },
                                Alignment.Center,
                            ) { Icon(Icons.Filled.Close, "Remove", tint = Color.White, modifier = Modifier.size(13.dp)) }
                        }
                    }
                }
            }
            // Transfer bar — shown while uploading.
            if (sending) {
                Column(Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 2.dp)) {
                    Text(transferLabel ?: "Sending…", color = muted, fontSize = 11.sp)
                    Spacer(Modifier.height(3.dp))
                    LinearProgressIndicator(
                        modifier = Modifier.fillMaxWidth().height(3.dp).clip(RoundedCornerShape(2.dp)),
                        color = goldInk, trackColor = glassFill,
                    )
                }
            }
            // The input bar — ONE floating glass panel (attach · text · send), like the feed dock.
            Row(
                Modifier.fillMaxWidth()
                    .clip(RoundedCornerShape(28.dp))
                    .background(bg2.copy(alpha = 0.84f))
                    .background(Brush.verticalGradient(listOf(Color.White.copy(alpha = if (heyLight) 0.18f else 0.07f), Color.White.copy(alpha = 0.02f))))
                    .border(1.dp, glassBorder, RoundedCornerShape(28.dp))
                    .padding(horizontal = 4.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = { picker.launch("*/*") }, enabled = !sending, modifier = Modifier.size(44.dp)) {
                    if (sending) CircularProgressIndicator(Modifier.size(20.dp), color = goldInk, strokeWidth = 2.dp)
                    else Icon(Icons.Filled.AttachFile, "Attach", tint = muted)
                }
                BasicTextField(
                    value = input, onValueChange = { input = it },
                    modifier = Modifier.weight(1f),
                    textStyle = androidx.compose.ui.text.TextStyle(color = ink, fontSize = 15.sp),
                    cursorBrush = SolidColor(Gold), maxLines = 5,
                    decorationBox = { inner ->
                        Box(Modifier.padding(end = 8.dp, top = 12.dp, bottom = 12.dp)) {
                            if (input.isEmpty()) Text("Message…", color = muted, fontSize = 15.sp)
                            inner()
                        }
                    },
                )
                Spacer(Modifier.width(4.dp))
                val canSend = input.isNotBlank() || staged.isNotEmpty()
                FloatingActionButton(
                    onClick = { sendStaged() },
                    containerColor = if (canSend) Gold else Gold.copy(alpha = 0.72f),
                    contentColor = Navy,
                    shape = CircleShape,
                    elevation = FloatingActionButtonDefaults.elevation(0.dp, 0.dp, 0.dp, 0.dp),
                    modifier = Modifier.size(42.dp),
                ) { Icon(Icons.AutoMirrored.Filled.Send, "Send") }
            }
        }
        if (showCryptoSend) TipSheet(chat.id, chat.name) { showCryptoSend = false }
        if (showChatInfo) ChatInfoSheet(
            chat, msgs,
            onViewProfile = { profileDid = chat.id },
            onCall = { CallManager.startCall(chat.id, chat.name) },
            onTip = { showCryptoSend = true },
            onBlocked = onBack,
            onClose = { showChatInfo = false },
        )
        profileDid?.let { d -> UserProfileScreen(d, onBack = { profileDid = null }, onMessage = { profileDid = null }) }
    }

    // Emoji reaction picker.
    if (reactTarget != null) {
        androidx.compose.ui.window.Dialog(onDismissRequest = { reactTarget = null }) {
            Row(Modifier.glass(22.dp).padding(10.dp, 8.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                listOf("👍", "❤️", "😂", "😮", "😢", "🎉", "🙏", "🔥").forEach { e ->
                    Text(e, fontSize = 26.sp, modifier = Modifier
                        .clip(androidx.compose.foundation.shape.CircleShape)
                        .clickable {
                            val id = reactTarget!!; reactTarget = null
                            scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.reactToMessage(chat, id, e) } }; reload() }
                        }
                        .padding(6.dp))
                }
            }
        }
    }
    // Hold-on-own-message → the standard Hey sheet: Edit or Delete.
    actionTarget?.let { m ->
        AlertDialog(
            onDismissRequest = { actionTarget = null },
            title = { Text("Message", color = ink) },
            text = {
                Column {
                    TextButton(
                        onClick = { actionTarget = null; editTarget = m },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(Icons.Filled.Edit, null, tint = goldInk, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(10.dp))
                        Text("Edit", color = ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                    }
                    TextButton(
                        onClick = { actionTarget = null; deleteTarget = m.id },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(Icons.Filled.Delete, null, tint = Like, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(10.dp))
                        Text("Delete", color = Like, fontSize = 15.sp, modifier = Modifier.weight(1f))
                    }
                }
            },
            confirmButton = {},
            dismissButton = { TextButton(onClick = { actionTarget = null }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
    // Edit in place — sent to everyone in the chat.
    editTarget?.let { m ->
        var editText by remember(m.id) { mutableStateOf(m.text) }
        AlertDialog(
            onDismissRequest = { editTarget = null },
            icon = { Icon(Icons.Filled.Edit, null, tint = goldInk) },
            title = { Text("Edit message", color = ink) },
            text = {
                OutlinedTextField(
                    value = editText, onValueChange = { editText = it },
                    modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                    textStyle = androidx.compose.ui.text.TextStyle(color = ink, fontSize = 15.sp),
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        val t = editText.trim()
                        editTarget = null
                        if (t.isNotBlank() && t != m.text) {
                            scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.editMessage(chat, m.id, t) } }; reload() }
                        }
                    },
                ) { Text("Save", color = goldInk, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { editTarget = null }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
    // Delete for everyone (tombstone) — confirm in the same Hey style.
    if (deleteTarget != null) {
        AlertDialog(
            onDismissRequest = { deleteTarget = null },
            icon = { Icon(Icons.Filled.Delete, null, tint = Like) },
            title = { Text("Delete message?", color = ink) },
            text = { Text("Removed for everyone in this chat.", color = muted, fontSize = 13.sp) },
            confirmButton = {
                TextButton(
                    onClick = {
                        val id = deleteTarget!!; deleteTarget = null
                        scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.deleteMessage(chat, id) } }; reload() }
                    },
                ) { Text("Delete", color = Like, fontWeight = FontWeight.Bold) }
            },
            dismissButton = { TextButton(onClick = { deleteTarget = null }) { Text("Cancel", color = muted) } },
            containerColor = sheetBg,
        )
    }
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
private fun Bubble(
    m: Msg, isGroup: Boolean, reactions: List<MsgReaction>,
    onLongPress: () -> Unit, onReact: (String) -> Unit,
) {
    val align = if (m.mine) Alignment.End else Alignment.Start
    Column(Modifier.fillMaxWidth().padding(vertical = 3.dp), horizontalAlignment = align) {
        if (isGroup && !m.mine && m.sender.isNotBlank()) {
            Text(m.sender, color = goldInk, fontSize = 11.sp, modifier = Modifier.padding(start = 6.dp, bottom = 1.dp))
        }
        val bubbleShape = if (m.mine) RoundedCornerShape(18.dp, 18.dp, 4.dp, 18.dp)
                          else RoundedCornerShape(18.dp, 18.dp, 18.dp, 4.dp)
        Column(
            Modifier.widthIn(max = 300.dp)
                .clip(bubbleShape)
                .background(if (m.mine) Gold else bubbleIn)
                .border(if (!m.mine && heyLight) 1.dp else 0.dp, glassBorder, bubbleShape)
                .combinedClickable(onClick = {}, onLongClick = onLongPress)
                .padding(if (m.attachments.isNotEmpty() && m.text.isBlank()) 6.dp else 12.dp, if (m.attachments.isNotEmpty() && m.text.isBlank()) 6.dp else 8.dp)
        ) {
            m.attachments.forEach { AttachmentView(it, m.mine) }
            if (m.text.isNotBlank()) {
                if (m.attachments.isNotEmpty()) Spacer(Modifier.height(6.dp))
                Text(m.text, color = if (m.mine) Navy else ink, fontSize = 15.sp)
            }
            // Time + read ticks inside the bubble, bottom-right (Telegram-style).
            if (m.ts > 0) Row(Modifier.align(Alignment.End).padding(top = 3.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(clockTime(m.ts), color = if (m.mine) Navy.copy(alpha = 0.6f) else muted, fontSize = 10.sp)
                if (m.mine) {
                    Spacer(Modifier.width(3.dp))
                    Icon(Icons.Filled.DoneAll, "Sent", tint = Navy.copy(alpha = 0.6f), modifier = Modifier.size(13.dp))
                }
            }
        }
        // Reaction chips (tap to toggle yours).
        if (reactions.isNotEmpty()) {
            Row(Modifier.padding(top = 3.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                reactions.groupBy { it.emoji }.forEach { (emoji, who) ->
                    Box(Modifier.clip(RoundedCornerShape(11.dp)).background(glassFill).border(1.dp, glassBorder, RoundedCornerShape(11.dp))
                        .clickable { onReact(emoji) }.padding(7.dp, 2.dp)) {
                        Text("$emoji ${who.size}", color = ink, fontSize = 12.sp)
                    }
                }
            }
        }
    }
}

/** Renders one attachment: images inline (fetched + decrypted), others as a
 *  tappable row that opens the decrypted file in an external app. */
@Composable
private fun AttachmentView(att: Attachment, mine: Boolean) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    if (att.isImage) {
        var bytes by remember(att.raw) { mutableStateOf<ByteArray?>(null) }
        var bmp by remember(att.raw) { mutableStateOf<Bitmap?>(null) }
        var failed by remember(att.raw) { mutableStateOf(false) }
        var attempt by remember(att.raw) { mutableStateOf(0) }
        var full by remember(att.raw) { mutableStateOf(false) }
        LaunchedEffect(att.raw, attempt) {
            failed = false
            val raw = withContext(Dispatchers.IO) { runCatching { HeyApi.fetchAttachment(att).takeIf { it.isNotEmpty() } }.getOrNull() }
            val b = raw?.let { withContext(Dispatchers.IO) { runCatching { BitmapFactory.decodeByteArray(it, 0, it.size) }.getOrNull() } }
            if (b != null) { bytes = raw; bmp = b } else failed = true
        }
        val b = bmp
        when {
            b != null -> Image(
                b.asImageBitmap(), att.name,
                Modifier.widthIn(max = 240.dp).clip(RoundedCornerShape(12.dp)).clickable { full = true },
                contentScale = ContentScale.FillWidth
            )
            failed -> Box(Modifier.size(200.dp, 130.dp).clip(RoundedCornerShape(12.dp)).background(glassFill).clickable { attempt++ }, Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(Icons.Filled.Refresh, null, tint = goldInk, modifier = Modifier.size(28.dp))
                    Spacer(Modifier.height(6.dp))
                    Text("Tap to load photo", color = if (mine) Navy else ink, fontSize = 12.sp)
                }
            }
            else -> Box(Modifier.size(200.dp, 130.dp).clip(RoundedCornerShape(12.dp)).background(glassFill), Alignment.Center) {
                CircularProgressIndicator(color = goldInk, strokeWidth = 2.dp)
            }
        }
        if (full) bytes?.let { FullImageViewer(it, att.name) { full = false } }
    } else {
        Row(
            Modifier.clip(RoundedCornerShape(12.dp)).background(if (mine) Color(0x22000000) else glassFill)
                .padding(10.dp, 8.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            // Tap the icon/name to play/open in an external app.
            Row(
                Modifier.clickable { scope.launch { openAttachment(ctx, att) } },
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(if (att.isVideo) Icons.Filled.PlayArrow else Icons.Filled.Description, null, tint = if (mine) Navy else goldInk)
                Spacer(Modifier.width(8.dp))
                Column {
                    Text(att.name.ifBlank { "file" }, color = if (mine) Navy else ink, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(humanSize(att.size), color = if (mine) Navy.copy(alpha = 0.7f) else muted, fontSize = 11.sp)
                }
            }
            Spacer(Modifier.width(10.dp))
            // Explicit Save/Download — bigger, with a live % while the bytes pull in.
            var saving by remember(att.raw) { mutableStateOf(false) }
            var pct by remember(att.raw) { mutableStateOf(-1) }
            if (saving) {
                LaunchedEffect(Unit) {
                    while (saving) {
                        pct = withContext(Dispatchers.IO) { HeyApi.attachmentProgress("") }
                        kotlinx.coroutines.delay(250)
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(modifier = Modifier.size(24.dp), strokeWidth = 2.5.dp, color = if (mine) Navy else goldInk)
                    Spacer(Modifier.width(6.dp))
                    Text(if (pct in 0..100) "$pct%" else "…", color = if (mine) Navy else ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                }
            } else {
                IconButton(
                    onClick = {
                        saving = true; pct = -1
                        scope.launch {
                            val ok = saveAttachment(ctx, att)
                            saving = false
                            val where = if (att.isVideo || att.mime.startsWith("video/")) "Movies/Hey" else "Downloads/Hey"
                            android.widget.Toast.makeText(
                                ctx,
                                if (ok) "Saved to $where" else "Couldn't save — sender may be offline",
                                android.widget.Toast.LENGTH_SHORT
                            ).show()
                        }
                    },
                    modifier = Modifier.size(48.dp)
                ) {
                    Icon(Icons.Filled.Download, "Save", tint = if (mine) Navy else goldInk, modifier = Modifier.size(28.dp))
                }
            }
        }
    }
}

/** Full-screen pinch-to-zoom viewer for a decrypted chat photo, with a save-to-
 *  gallery button. Works on the raw bytes (chat attachments aren't a public CID). */
@Composable
private fun FullImageViewer(bytes: ByteArray, name: String, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val bmp = remember(bytes) { runCatching { BitmapFactory.decodeByteArray(bytes, 0, bytes.size) }.getOrNull() }
    androidx.compose.ui.window.Dialog(
        onDismissRequest = onClose,
        properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)
    ) {
        var scale by remember { mutableStateOf(1f) }
        var offset by remember { mutableStateOf(Offset.Zero) }
        val state = rememberTransformableState { z, p, _ ->
            scale = (scale * z).coerceIn(1f, 5f)
            offset = if (scale > 1f) offset + p else Offset.Zero
        }
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.96f)), Alignment.Center) {
            bmp?.let {
                Image(
                    it.asImageBitmap(), name, contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxSize()
                        .graphicsLayer(scaleX = scale, scaleY = scale, translationX = offset.x, translationY = offset.y)
                        .transformable(state)
                )
            }
            Row(Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(8.dp)) {
                IconButton(onClick = {
                    scope.launch {
                        val ok = withContext(Dispatchers.IO) { saveImageToGallery(ctx, bytes, name) }
                        android.widget.Toast.makeText(ctx, if (ok) "Saved to Photos" else "Couldn't save", android.widget.Toast.LENGTH_SHORT).show()
                    }
                }) { Icon(Icons.Filled.Download, "Save", tint = Color.White) }
                IconButton(onClick = onClose) { Icon(Icons.Filled.Close, "Close", tint = Color.White) }
            }
            Text("Pinch to zoom", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp,
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(16.dp))
        }
    }
}

/** Save image bytes into the device gallery (Pictures/Hey) via MediaStore. */
private fun saveImageToGallery(ctx: android.content.Context, bytes: ByteArray, name: String): Boolean = runCatching {
    val base = name.ifBlank { "hey" }.substringBeforeLast('.', name.ifBlank { "hey" }).ifBlank { "hey" }
        .replace(Regex("[^A-Za-z0-9_-]"), "_")
    val stamp = android.os.SystemClock.elapsedRealtime()
    val values = android.content.ContentValues().apply {
        put(android.provider.MediaStore.Images.Media.DISPLAY_NAME, "${base}_$stamp.webp")
        put(android.provider.MediaStore.Images.Media.MIME_TYPE, "image/webp")
        if (Build.VERSION.SDK_INT >= 29) put(android.provider.MediaStore.Images.Media.RELATIVE_PATH, android.os.Environment.DIRECTORY_PICTURES + "/Hey")
    }
    val resolver = ctx.contentResolver
    val uri = resolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values) ?: return@runCatching false
    resolver.openOutputStream(uri)?.use { it.write(bytes) } ?: return@runCatching false
    true
}.getOrDefault(false)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AddContactSheet(onClose: () -> Unit, onStartChat: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    var link by remember { mutableStateOf("") }
    var qr by remember { mutableStateOf<Bitmap?>(null) }
    var input by remember { mutableStateOf("") }
    var status by remember { mutableStateOf("") }
    var following by remember { mutableStateOf<List<Follow>>(emptyList()) }
    // After a successful scan, auto-connect — scanning alone used to just fill the
    // field, which read as "nothing happened". Consumed by a LaunchedEffect below
    // (placed after submit() so it's in scope).
    var pendingScan by remember { mutableStateOf(false) }
    // QrLink.fromScan turns a scanned follow-QR back into a hey:follow: link.
    val scanner = rememberLauncherForActivityResult(ScanContract()) { r -> r.contents?.let { input = QrLink.fromScan(it); pendingScan = true } }
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            // Share the SAME compact friend-link QR everywhere — it's DM-capable
            // and scannable (the chat-invite QR was too dense to read reliably).
            runCatching { link = HeyApi.friendLink(); if (link.isNotBlank()) qr = qrBitmap(QrLink.toQr(link)) }
            following = runCatching { HeyApi.following() }.getOrDefault(emptyList())
        }
    }
    fun startWith(did: String) {
        status = "Starting…"
        scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.startChat(did) } }; onStartChat(did) }
    }
    fun submit() {
        // A scanned QR was already normalized; normalize a pasted one too.
        val v = QrLink.fromScan(input.trim())
        if (v.isEmpty()) { status = "Paste a friend link or invite"; return }
        status = "Connecting…"
        scope.launch {
            when {
                v.startsWith("hey:follow:") -> {
                    // The friend link carries the PQ keys → follow bootstraps a DM,
                    // then we open the chat. Same link you got from following.
                    val res = withContext(Dispatchers.IO) { runCatching { HeyApi.follow(v) }.getOrNull() }
                    val did = res?.optString("did").orEmpty()
                    if (res != null && !res.has("error") && did.isNotEmpty()) {
                        withContext(Dispatchers.IO) { runCatching { HeyApi.startChat(did) } }; onStartChat(did)
                    } else status = "Failed: ${res?.optString("error") ?: "invalid link"}"
                }
                v.startsWith("hey-invite:") -> {
                    val res = withContext(Dispatchers.IO) { runCatching { HeyApi.acceptInvite(v) }.getOrNull() }
                    val did = res?.optString("did").orEmpty()
                    if (res != null && !res.has("error")) { if (did.isNotEmpty()) onStartChat(did) else onClose() }
                    else status = "Failed: ${res?.optString("error") ?: "invalid"}"
                }
                v.startsWith("did:") -> status = "That's a DID — paste a Hey friend link instead."
                // Don't feed unrecognized text to the engine (that caused the
                // "invite base64" error from a misread/oversized QR).
                else -> status = "Unrecognized — paste a Hey friend link or scan a Hey QR."
            }
        }
    }
    // Auto-submit once a scan lands (submit is now in scope).
    LaunchedEffect(pendingScan) { if (pendingScan) { pendingScan = false; submit() } }
    // Long sheet (follow list + paste field + invite QR): force full expansion so the
    // full-width QR at the bottom renders completely instead of being clipped by a
    // partially-expanded sheet — matches the profile "Show my QR" sheet.
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(onDismissRequest = onClose, sheetState = sheetState, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("New chat", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text("Message someone you follow, paste their Hey friend link, or share your invite.", color = muted, fontSize = 13.sp)

            // Anyone you follow is already DM-capable (their link carried the keys).
            if (following.isNotEmpty()) {
                Spacer(Modifier.height(16.dp))
                Text("People you follow", color = muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(6.dp))
                following.forEach { f -> PersonRow(f.did, onClick = { startWith(f.did) }) }
                Spacer(Modifier.height(8.dp))
                androidx.compose.material3.HorizontalDivider(color = glassBorder)
            }

            Spacer(Modifier.height(16.dp))
            Text("Add by link or invite", color = muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(10.dp))
            OutlinedTextField(
                value = input, onValueChange = { input = it }, singleLine = true, maxLines = 1,
                placeholder = { Text("Paste a friend link or invite…", color = muted, fontSize = 13.sp) },
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink, fontSize = 13.sp),
            )
            if (input.length > 24) {
                Spacer(Modifier.height(4.dp))
                Text("✓ Link ready (${input.length} chars)", color = good, fontSize = 11.sp)
            }
            Spacer(Modifier.height(12.dp))
            Row {
                OutlinedButton(onClick = { scanner.launch(ScanOptions().setDesiredBarcodeFormats(ScanOptions.QR_CODE).setOrientationLocked(false).setBeepEnabled(false).setPrompt("Scan a Hey QR").setCaptureActivity(PortraitCaptureActivity::class.java)) }) {
                    Icon(Icons.Filled.QrCodeScanner, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Scan", color = ink)
                }
                Spacer(Modifier.width(12.dp))
                Button(onClick = { submit() }, colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    Text("Start chat", fontWeight = FontWeight.Bold)
                }
            }
            if (status.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(status, color = muted, fontSize = 13.sp) }

            Spacer(Modifier.height(20.dp))
            androidx.compose.material3.HorizontalDivider(color = glassBorder)
            Spacer(Modifier.height(16.dp))
            Text("Or share your invite", color = muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(4.dp))
            Text("Best: Share the link. The QR is dense (it carries your encryption key) — scan close, in good light.", color = muted, fontSize = 11.sp)
            Spacer(Modifier.height(10.dp))
            Box(Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(16.dp)).background(Color.White).padding(10.dp), Alignment.Center) {
                when {
                    qr != null -> Image(qr!!.asImageBitmap(), "invite QR", Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                    link.isBlank() -> CircularProgressIndicator(color = Navy)
                    else -> Text("Use Share / Copy below", color = Navy, fontSize = 13.sp)
                }
            }
            Spacer(Modifier.height(10.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
                Button(onClick = { shareText(ctx, link) }, enabled = link.isNotBlank(),
                    colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                    Icon(Icons.Filled.Share, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Share link", fontWeight = FontWeight.Bold)
                }
                Spacer(Modifier.width(10.dp))
                OutlinedButton(onClick = { clipboard.setText(AnnotatedString(link)) }, enabled = link.isNotBlank()) {
                    Icon(Icons.Filled.ContentCopy, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Copy", color = ink)
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** Create a group: pick a name + members from your existing contacts. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NewGroupSheet(onClose: () -> Unit, onCreated: () -> Unit) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var contacts by remember { mutableStateOf<List<Chat>>(emptyList()) }
    val selected = remember { mutableStateListOf<String>() }
    var busy by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf("") }
    LaunchedEffect(Unit) {
        contacts = withContext(Dispatchers.IO) { runCatching { HeyApi.chats().filter { !it.isGroup } }.getOrDefault(emptyList()) }
    }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = sheetBg) {
        Column(Modifier.fillMaxWidth().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("New group", color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = name, onValueChange = { name = it }, singleLine = true,
                placeholder = { Text("Group name", color = muted) },
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink),
            )
            Spacer(Modifier.height(14.dp))
            Text("Add members", color = muted, fontSize = 13.sp)
            Spacer(Modifier.height(6.dp))
            if (contacts.isEmpty()) {
                Text("Add some contacts first — then you can group them.", color = muted, fontSize = 13.sp)
            } else {
                contacts.forEach { c ->
                    val on = selected.contains(c.id)
                    Row(
                        Modifier.fillMaxWidth().padding(vertical = 4.dp).glass(12.dp)
                            .clickable { if (on) selected.remove(c.id) else selected.add(c.id) }.padding(10.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Box(Modifier.size(38.dp).clip(RoundedCornerShape(19.dp)).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                            Text(c.name.take(1).uppercase(), color = Navy, fontWeight = FontWeight.Bold)
                        }
                        Spacer(Modifier.width(10.dp))
                        Text(c.name, color = ink, fontSize = 15.sp, modifier = Modifier.weight(1f))
                        if (on) Icon(Icons.Filled.CheckCircle, "selected", tint = goldInk)
                        else Icon(Icons.Filled.RadioButtonUnchecked, "select", tint = muted)
                    }
                }
            }
            if (status.isNotBlank()) { Spacer(Modifier.height(8.dp)); Text(status, color = Like, fontSize = 13.sp) }
            Spacer(Modifier.height(16.dp))
            Button(
                onClick = {
                    val n = name.trim()
                    if (n.isEmpty()) { status = "Name the group"; return@Button }
                    if (selected.isEmpty()) { status = "Pick at least one member"; return@Button }
                    busy = true; status = ""
                    val members = selected.toList()
                    scope.launch {
                        val id = withContext(Dispatchers.IO) { runCatching { HeyApi.createGroup(n, members) }.getOrNull() }
                        busy = false
                        if (id != null) { onCreated(); onClose() } else status = "Couldn't create group"
                    }
                },
                enabled = !busy, modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
            ) {
                if (busy) CircularProgressIndicator(Modifier.size(20.dp), color = Navy, strokeWidth = 2.dp)
                else Text("Create group", fontWeight = FontWeight.Bold)
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** Full-screen lock shown when the optional app lock is on. Auto-prompts once;
 *  a button re-triggers if the user dismissed the system sheet. */
@Composable
private fun LockScreen(onRestore: () -> Unit, onUnlock: () -> Unit) {
    LaunchedEffect(Unit) { onUnlock() }
    Column(Modifier.fillMaxSize().padding(32.dp), Arrangement.Center, Alignment.CenterHorizontally) {
        Box(Modifier.size(96.dp).clip(CircleShape).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
            Icon(Icons.Filled.Lock, null, tint = Navy, modifier = Modifier.size(46.dp))
        }
        Spacer(Modifier.height(20.dp))
        Text("Hey is locked", color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(6.dp))
        Text("Verify it's you to open your data.", color = muted, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        Spacer(Modifier.height(24.dp))
        Button(onClick = onUnlock, colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
            Icon(Icons.Filled.Fingerprint, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Unlock", fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(14.dp))
        // Always-available escape: if the device lock changed/was removed the
        // hardware key is permanently invalidated and biometric unlock can never
        // succeed. A user who still has their recovery phrase must never be bricked.
        TextButton(onClick = onRestore) {
            Text("Restore from recovery phrase", color = muted, fontSize = 13.sp)
        }
    }
}

// ── onboarding (own your identity + data) ────────────────────────────────────

/** First-run welcome: a SWIPEABLE intro (so text + illustration never need scrolling)
 *  with the create-new / restore choice ALWAYS visible at the bottom. Shown before
 *  the runtime starts, so a restore can supply the seed. */
@Composable
private fun WelcomeFlow(onCreateNew: () -> Unit, onRestore: (String) -> Unit) {
    var restoreMode by remember { mutableStateOf(false) }
    if (restoreMode) { RestoreScreen(onBack = { restoreMode = false }, onRestore = onRestore); return }
    val pager = rememberPagerState(pageCount = { 3 })
    Column(Modifier.fillMaxSize().padding(24.dp, 16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        HorizontalPager(state = pager, modifier = Modifier.weight(1f).fillMaxWidth()) { page ->
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center
            ) {
                when (page) {
                    0 -> {
                        Box(contentAlignment = Alignment.Center, modifier = Modifier.size(150.dp)) {
                            Box(Modifier.matchParentSize().clip(CircleShape).background(Brush.radialGradient(listOf(Gold.copy(alpha = 0.40f), Color.Transparent))))
                            Text("👋", fontSize = 76.sp)
                        }
                        Spacer(Modifier.height(8.dp))
                        Text("Hey", color = goldInk, fontSize = 56.sp, fontWeight = FontWeight.Bold)
                        Spacer(Modifier.height(6.dp))
                        Text("a warm little corner of the internet that's truly yours 💛", color = ink, fontSize = 16.sp, fontWeight = FontWeight.Medium, textAlign = androidx.compose.ui.text.style.TextAlign.Center, lineHeight = 22.sp)
                        Spacer(Modifier.height(12.dp))
                        Text("No ads, no snooping, no strangers in your data — just you and the people you love, safe on your own device. 🌿", color = muted, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center, lineHeight = 21.sp)
                    }
                    1 -> {
                        Text("Yours, end to end", color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                        Spacer(Modifier.height(18.dp))
                        Column(Modifier.fillMaxWidth().glass().padding(16.dp)) {
                            OnbRow(Icons.Filled.Key, "A self-sovereign identity", "A did:key generated and held only on your phone.")
                            Spacer(Modifier.height(10.dp))
                            OnbRow(Icons.Filled.Lock, "End-to-end encrypted", "Post-quantum DMs + signed posts. No middleman can read them.")
                            Spacer(Modifier.height(10.dp))
                            OnbRow(Icons.Filled.CloudOff, "No servers, no accounts", "Your data lives with you and your friends — nowhere else.")
                        }
                    }
                    else -> {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Filled.Public, null, tint = goldInk, modifier = Modifier.size(26.dp))
                            Spacer(Modifier.width(8.dp)); Text("Powered by ElastOS", color = ink, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                        }
                        Spacer(Modifier.height(12.dp))
                        Text("ElastOS is a decentralized internet where you — not companies — own your identity, data, and money. Your phone is the node. One recovery phrase is your sovereign identity and wallet across the whole network.",
                            color = muted, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center, lineHeight = 21.sp)
                    }
                }
            }
        }
        Row(Modifier.padding(vertical = 12.dp), horizontalArrangement = Arrangement.Center) {
            repeat(3) { i ->
                Box(Modifier.padding(horizontal = 4.dp).size(if (pager.currentPage == i) 9.dp else 7.dp)
                    .clip(CircleShape).background(if (pager.currentPage == i) goldInk else muted.copy(alpha = 0.4f)))
            }
        }
        Button(onClick = onCreateNew, modifier = Modifier.fillMaxWidth().height(54.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
            Text("Create new identity", fontWeight = FontWeight.Bold, fontSize = 16.sp)
        }
        Spacer(Modifier.height(10.dp))
        OutlinedButton(onClick = { restoreMode = true }, modifier = Modifier.fillMaxWidth().height(50.dp)) {
            Icon(Icons.Filled.Key, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("I have a recovery phrase", color = ink)
        }
        Spacer(Modifier.height(14.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) { ThemeToggle() }
        Spacer(Modifier.height(8.dp))
    }
}

/** Gate a sensitive action (recovery-phrase reveal) behind a fresh hardware auth —
 *  fingerprint/face/PIN, verified in the TEE/StrongBox. If the device has NO screen lock at all,
 *  refuse and tell the user to set one up; never silently proceed. */
private fun requireAuth(
    activity: androidx.fragment.app.FragmentActivity?, ctx: android.content.Context, action: () -> Unit,
) {
    if (activity != null && AppLock.available(ctx)) AppLock.prompt(activity) { ok -> if (ok) action() }
    else android.widget.Toast.makeText(ctx, "Set up a fingerprint or screen lock in Android settings to do this.", android.widget.Toast.LENGTH_LONG).show()
}

/** Priority 2 — the SEND gate. Wallet/BEAM sends are ALWAYS TEE-confirmed, but with
 *  exactly ONE prompt: the per-spend hardware biometric inside SpendAuth.spendGrant
 *  (StrongBox/TEE P-256 sign the Rust guard verifies). So we DON'T stack a second
 *  AppLock prompt here — we just proceed to `send`, which performs that single scan
 *  (and, on the first ever send, also lazily enrolls the spend key in that same scan).
 *  On a device with NO secure lock at all there is no TEE to confirm with, so a money
 *  send is REFUSED (never a silent no-auth send) — the user must set up a screen lock. */
private fun spendGate(
    activity: androidx.fragment.app.FragmentActivity?, ctx: android.content.Context, send: () -> Unit,
) {
    if (activity != null && SpendAuth.available(ctx)) send() // the single TEE prompt happens in spendGrant
    else android.widget.Toast.makeText(ctx, "Set up a fingerprint or screen lock in Android settings to send funds.", android.widget.Toast.LENGTH_LONG).show()
}

/** While composed, hardens the OWNING window for a sensitive surface (recovery phrase, money
 *  confirm): FLAG_SECURE blocks screenshots / screen-recording / the recents thumbnail, and
 *  filterTouchesWhenObscured drops taps that pass through an overlay another app draws on top
 *  (tap-jacking). Resolves to the Dialog window inside a Dialog or the Activity window otherwise;
 *  both are reverted on exit. */
@Composable
private fun SecureWindow() {
    val view = androidx.compose.ui.platform.LocalView.current
    DisposableEffect(view) {
        val dialogWindow = (view.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window
        val window = dialogWindow ?: (view.context as? android.app.Activity)?.window
        window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
        view.filterTouchesWhenObscured = true
        onDispose {
            if (dialogWindow == null) window?.clearFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
            view.filterTouchesWhenObscured = false
        }
    }
}

/**
 * H2.2 — recovery-phrase-recorded gate, shown before every enableVault seal (the
 * onboarding require-unlock branch, the Settings App-lock toggle ON, the migration
 * offer). Shows the words (screenshot-blocked) + a clear loss warning, then offers two
 * ways forward:
 *   • PRIMARY "Verify" — re-enter `checkCount` randomly-chosen words; `onConfirmed`
 *     fires only when every challenged word matches (case/space-insensitive).
 *   • SECONDARY "Skip verification" (Priority 4, user ask) — enabled ONLY after the user
 *     ticks an explicit "I understand … no recovery" checkbox; skip proceeds straight to
 *     `onConfirmed` (the seal).
 * `onCancel` on dismiss returns WITHOUT sealing (no brick). Pure UI — it does not touch
 * the vault; the caller's `onConfirmed` performs the atomic enableVault seal.
 */
@Composable
private fun RecordPhraseGate(
    phrase: String, onConfirmed: () -> Unit, onCancel: () -> Unit, checkCount: Int = 3,
) {
    val words = remember(phrase) { phrase.trim().split(Regex("\\s+")).filter { it.isNotBlank() } }
    // Stable random positions to challenge (1-based for display).
    val challenge = remember(phrase) {
        if (words.size < checkCount) emptyList()
        else words.indices.shuffled().take(checkCount).sorted()
    }
    var revealed by remember { mutableStateOf(false) }
    val answers = remember(phrase) { mutableStateListOf(*Array(challenge.size) { "" }) }
    var err by remember { mutableStateOf<String?>(null) }
    // Priority 4: the user may SKIP verification after explicitly accepting the loss risk.
    var skipAck by remember { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = onCancel,
        icon = { Icon(Icons.Filled.Key, null, tint = goldInk) },
        title = { Text(if (!revealed) "Save your recovery phrase" else "Confirm you saved it", color = ink) },
        text = {
            Column {
                SecureWindow() // block screenshots / recents while the phrase is on screen
                if (!revealed) {
                    Text("These 12 words ARE your account — they recover your Hey identity, your Elastos DID and your wallets. If you ever change this phone's lock, they are the ONLY way back in. Write them down offline now; never share or screenshot.",
                        color = muted, fontSize = 13.sp, lineHeight = 19.sp)
                    Spacer(Modifier.height(14.dp))
                    Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Color.Black.copy(alpha = 0.13f)).padding(14.dp)) {
                        Text(words.mapIndexed { i, w -> "${i + 1}. $w" }.joinToString("   "),
                            color = ink, fontSize = 15.sp, fontFamily = mono, lineHeight = 26.sp)
                    }
                    Spacer(Modifier.height(14.dp))
                    // Skip-verification acknowledgement: only after the user explicitly ticks
                    // the loss warning does the "skip" button below become enabled.
                    Row(verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth().clickable { skipAck = !skipAck }) {
                        Checkbox(checked = skipAck, onCheckedChange = { skipAck = it },
                            colors = CheckboxDefaults.colors(checkedColor = Gold, checkmarkColor = Navy))
                        Text("I understand: if I lose this phrase I lose my funds and there is no recovery.",
                            color = muted, fontSize = 12.sp, lineHeight = 17.sp)
                    }
                } else {
                    Text("Type the requested words to confirm you recorded them.", color = muted, fontSize = 13.sp)
                    Spacer(Modifier.height(10.dp))
                    challenge.forEachIndexed { i, pos ->
                        OutlinedTextField(
                            value = answers[i], onValueChange = { answers[i] = it; err = null },
                            placeholder = { Text("Word #${pos + 1}", color = muted) }, singleLine = true,
                            modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                            textStyle = androidx.compose.ui.text.TextStyle(color = ink),
                            keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                                keyboardType = androidx.compose.ui.text.input.KeyboardType.Password,
                                autoCorrect = false),
                        )
                        Spacer(Modifier.height(8.dp))
                    }
                    err?.let { Text(it, color = Color(0xFFE57373), fontSize = 12.sp) }
                }
            }
        },
        confirmButton = {
            if (!revealed) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    // Secondary: skip verification (enabled only once the loss risk is ticked).
                    TextButton(
                        enabled = skipAck,
                        onClick = onConfirmed, // explicit acceptance → straight to the seal
                    ) { Text("Skip verification", color = if (skipAck) ink else muted, fontSize = 13.sp) }
                    Spacer(Modifier.width(4.dp))
                    // Primary: verify by re-entering N words (or proceed if too few to challenge).
                    TextButton(onClick = {
                        if (challenge.isEmpty()) onConfirmed() // <checkCount words (legacy): can't challenge, but it was shown
                        else revealed = true
                    }) { Text(if (challenge.isEmpty()) "I've written it down" else "Verify", color = goldInk, fontWeight = FontWeight.Bold) }
                }
            } else {
                TextButton(onClick = {
                    val ok = challenge.indices.all {
                        answers[it].trim().equals(words[challenge[it]], ignoreCase = true)
                    }
                    if (ok) onConfirmed() else err = "Those words don't match. Check your written copy."
                }) { Text("Confirm", color = goldInk, fontWeight = FontWeight.Bold) }
            }
        },
        dismissButton = { TextButton(onClick = onCancel) { Text("Cancel", color = muted) } },
        containerColor = sheetBg,
    )
}

/** Restore an existing account from its 12-word phrase — re-derives did:key,
 *  did:elastos and wallets on this device. Validated in Rust before we commit. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RestoreScreen(onBack: () -> Unit, onRestore: (String) -> Unit) {
    var phrase by remember { mutableStateOf("") }
    var err by remember { mutableStateOf("") }
    SecureWindow() // phrase being typed must not leak to screenshots / recents
    Column(Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        Spacer(Modifier.height(20.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = ink) }
            Text("Restore your account", color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(8.dp))
        Text("Enter your 12-word Hey recovery phrase. It re-derives your identity, your Elastos DID and your wallets on this device — nothing is uploaded.", color = muted, fontSize = 14.sp, lineHeight = 20.sp)
        Spacer(Modifier.height(18.dp))
        OutlinedTextField(
            value = phrase, onValueChange = { phrase = it; err = "" },
            placeholder = { Text("word1  word2  word3  …", color = muted) },
            modifier = Modifier.fillMaxWidth().height(140.dp), colors = glassFieldColors(),
            textStyle = androidx.compose.ui.text.TextStyle(color = ink),
            // Password keyboard + no autocorrect: keeps the recovery words out of the
            // IME's personalized dictionary / suggestion cache / keyboard cloud sync.
            keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                keyboardType = androidx.compose.ui.text.input.KeyboardType.Password,
                autoCorrectEnabled = false,
            ),
        )
        if (err.isNotBlank()) { Spacer(Modifier.height(10.dp)); Text(err, color = Like, fontSize = 13.sp) }
        Spacer(Modifier.height(18.dp))
        Button(
            onClick = {
                val p = phrase.trim().lowercase().replace(Regex("\\s+"), " ")
                if (!HeyApi.validMnemonic(p)) { err = "That doesn't look like a valid 12-word recovery phrase. Check the words, spelling and order."; return@Button }
                onRestore(p)
            },
            modifier = Modifier.fillMaxWidth().height(54.dp), colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
        ) { Text("Restore", fontWeight = FontWeight.Bold, fontSize = 16.sp) }
        Spacer(Modifier.height(12.dp))
        Text("It's the same 12 words you can import into official Elastos Essentials.", color = muted, fontSize = 12.sp)
    }
}

// Profile setup, shown once after a NEW identity is created (restore skips it —
// the profile syncs from the network). The welcome/intro + create-vs-restore
// choice live in WelcomeFlow, before the runtime starts.
@Composable
fun OnboardingScreen(did: String, onDone: () -> Unit) {
    var working by remember { mutableStateOf(false) }
    var nickname by remember { mutableStateOf("") }
    var bio by remember { mutableStateOf("") }
    var avatar by remember { mutableStateOf<Bitmap?>(null) }
    var avatarBytes by remember { mutableStateOf<ByteArray?>(null) }
    val ctx = LocalContext.current
    // Priority 1: app-lock is an OPT-IN first-run CHOICE (default OFF). `canLock` =
    // the device CAN hardware-seal (a secure lock exists). On a no-secure-lock device
    // (canLock==false) only "Open freely" is possible — it can never be sealed, so it
    // never bricks. `requireUnlock` is the user's selection; default = false (Open freely).
    val canLock = remember { IdentityVault.available(ctx) }
    var requireUnlock by remember { mutableStateOf(false) }
    // H2.2 gate: the phrase the user must record before the seal; null = gate not shown.
    var gatePhrase by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val pickAvatar = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val b = withContext(Dispatchers.IO) { runCatching { scaleWebp(ctx.contentResolver.openInputStream(uri)!!.use { it.readBytes() }, 512, 82) }.getOrNull() }
            if (b != null) { avatarBytes = b; avatar = BitmapFactory.decodeByteArray(b, 0, b.size) }
        }
    }
    Column(
        Modifier.fillMaxSize().padding(28.dp).verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Spacer(Modifier.height(48.dp))
        Box(contentAlignment = Alignment.Center, modifier = Modifier.size(96.dp)) {
            Box(Modifier.matchParentSize().clip(CircleShape).background(Brush.radialGradient(listOf(Gold.copy(alpha = 0.35f), Color.Transparent))))
            Text("👋", fontSize = 48.sp)
        }
        Spacer(Modifier.height(12.dp))
        Text("Set up your profile", color = ink, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text("This is how others will see you. You can change it anytime.", color = muted, fontSize = 13.sp,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Spacer(Modifier.height(18.dp))
            Box(contentAlignment = Alignment.BottomEnd) {
                Box(Modifier.size(96.dp).clip(RoundedCornerShape(48.dp)).background(Brush.linearGradient(listOf(Gold, Gold2)))
                    .clickable { pickAvatar.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }, Alignment.Center) {
                    avatar?.let { Image(it.asImageBitmap(), null, Modifier.fillMaxSize().clip(RoundedCornerShape(48.dp)), contentScale = ContentScale.Crop) }
                        ?: Icon(Icons.Filled.AddAPhoto, null, tint = Navy, modifier = Modifier.size(34.dp))
                }
            }
            Spacer(Modifier.height(16.dp))
            OutlinedTextField(value = nickname, onValueChange = { nickname = it },
                placeholder = { Text("Nickname", color = muted) }, singleLine = true,
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink))
            Spacer(Modifier.height(10.dp))
            OutlinedTextField(value = bio, onValueChange = { bio = it },
                placeholder = { Text("Short bio (optional)", color = muted) },
                modifier = Modifier.fillMaxWidth(), colors = glassFieldColors(),
                textStyle = androidx.compose.ui.text.TextStyle(color = ink))

            Spacer(Modifier.height(16.dp))
            // Your data is sandboxed + OS-encrypted already; the lock is an extra layer.
            Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Filled.Shield, null, tint = good, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Your data stays on this device", color = ink, fontWeight = FontWeight.SemiBold)
                }
                Spacer(Modifier.height(6.dp))
                Text("Hey is sandboxed from other apps and Android encrypts it at rest. Nothing is uploaded to a company.", color = muted, fontSize = 13.sp)
            }

            // Priority 1: two-option app-lock chooser (only on hardware-capable devices).
            // DEFAULT = "Open freely" (no biometric to open Hey or read messages); the
            // user can opt into "Require unlock each open" for high security. On a device
            // with no secure lock this whole block is hidden and we finish Open-freely.
            if (canLock) {
                Spacer(Modifier.height(14.dp))
                Text("How should Hey open?", color = ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.align(Alignment.Start))
                Spacer(Modifier.height(8.dp))
                LockChoiceCard(
                    selected = !requireUnlock,
                    icon = Icons.Filled.Bolt,
                    title = "Open freely",
                    badge = "Recommended",
                    body = "Convenient: no fingerprint to open Hey or read messages. Your recovery seed is protected by hardware but recoverable by malware on an unlocked, rooted phone.",
                    onClick = { requireUnlock = false },
                )
                Spacer(Modifier.height(10.dp))
                LockChoiceCard(
                    selected = requireUnlock,
                    icon = Icons.Filled.Fingerprint,
                    title = "Require unlock each open",
                    badge = null,
                    body = "A fingerprint/PIN each time you open Hey; your seed is sealed behind it in the Titan M / Knox Vault / TEE. We'll show your recovery phrase first — it's the only way back in if you ever change this phone's lock.",
                    onClick = { requireUnlock = true },
                )
            }

            Spacer(Modifier.height(24.dp))
            Button(
                onClick = {
                    working = true
                    scope.launch {
                        withContext(Dispatchers.IO) {
                            runCatching {
                                var avatarCid = ""
                                avatarBytes?.let { val t = HeyApi.uploadMedia(it, "image/webp", "avatar.webp"); avatarCid = t.optString("cid") }
                                HeyApi.setProfile(nickname.trim().ifBlank { "Hey user" }, bio.trim(), avatarCid)
                            }
                        }
                        // Priority 1: only the "Require unlock each open" choice seals the
                        // vault. "Open freely" (default) finishes WITHOUT enableVault — the
                        // seed stays under the existing no-auth hardware-wrapped StorageVault
                        // DEK (vault OFF, no biometric to open the app). H2.2: before any seal
                        // the user must see/record their phrase (RecordPhraseGate, skippable).
                        val act = ctx as? androidx.fragment.app.FragmentActivity
                        if (requireUnlock && canLock && act != null) {
                            // Fresh onboarding: the bare phrase is in RAM (unlockedSeed).
                            val phrase = HeyApi.unlockedSeed ?: HeyApi.recoveryPhrase()
                            if (phrase.isNullOrBlank()) { working = false; onDone(); return@launch } // legacy seed-only: can't gate, finish
                            gatePhrase = phrase // shows RecordPhraseGate; its onConfirmed seals
                        } else { working = false; onDone() }
                    }
                },
                enabled = !working, modifier = Modifier.fillMaxWidth().height(54.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
            ) {
                if (working) CircularProgressIndicator(color = Navy, modifier = Modifier.size(22.dp), strokeWidth = 2.dp)
                else Text("Start owning my data", fontWeight = FontWeight.Bold, fontSize = 16.sp)
            }
        Spacer(Modifier.height(40.dp))
    }
    // H2.2: show the recovery phrase + require re-entering N words BEFORE the seal. Only
    // on confirm do we enableVault (seal → verify → persist → setOn → delete). Cancel
    // returns to the form (vault not yet on; nothing deleted — no brick).
    gatePhrase?.let { phrase ->
        val act = ctx as? androidx.fragment.app.FragmentActivity
        RecordPhraseGate(
            phrase = phrase,
            onConfirmed = {
                gatePhrase = null
                if (act != null) enableVault(act, ctx, scope) { working = false; onDone() }
                else { working = false; onDone() }
            },
            onCancel = { gatePhrase = null; working = false }, // back to onboarding; not sealed
        )
    }
}

/** Priority 1 — a selectable app-lock option card (radio-style). Selected = gold ring +
 *  filled radio; the body line states the trade-off. Tapping anywhere selects it. */
@Composable
private fun LockChoiceCard(
    selected: Boolean,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    title: String,
    badge: String?,
    body: String,
    onClick: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .glass()
            .border(if (selected) 2.dp else 1.dp, if (selected) Gold else glassBorder, RoundedCornerShape(14.dp))
            .clickable { onClick() }
            .padding(14.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, null, tint = if (selected) goldInk else muted, modifier = Modifier.size(24.dp))
            Spacer(Modifier.width(10.dp))
            Text(title, color = ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            badge?.let {
                Box(Modifier.clip(RoundedCornerShape(8.dp)).background(Gold.copy(alpha = 0.18f)).padding(8.dp, 3.dp)) {
                    Text(it, color = goldInk, fontSize = 11.sp, fontWeight = FontWeight.Medium)
                }
                Spacer(Modifier.width(8.dp))
            }
            Icon(
                if (selected) Icons.Filled.RadioButtonChecked else Icons.Filled.RadioButtonUnchecked,
                null, tint = if (selected) Gold else muted, modifier = Modifier.size(22.dp),
            )
        }
        Spacer(Modifier.height(6.dp))
        Text(body, color = muted, fontSize = 12.sp, lineHeight = 17.sp)
    }
}

@Composable
private fun OnbRow(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, body: String) {
    Row(verticalAlignment = Alignment.Top) {
        Icon(icon, null, tint = Gold, modifier = Modifier.size(22.dp))
        Spacer(Modifier.width(12.dp))
        Column {
            Text(title, color = ink, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
            Text(body, color = muted, fontSize = 13.sp, lineHeight = 18.sp)
        }
    }
}

@Composable
private fun Badge2(text: String, color: Color) {
    Box(Modifier.clip(RoundedCornerShape(12.dp)).background(glassFill).border(1.dp, glassBorder, RoundedCornerShape(12.dp)).padding(10.dp, 5.dp)) {
        Text(text, color = color, fontSize = 12.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun SecRow(k: String, v: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
        Text(k, color = muted, fontSize = 13.sp, modifier = Modifier.width(110.dp))
        Text(v, color = ink, fontSize = 13.sp)
    }
}

/** Always-on background delivery: shows whether Hey is exempt from battery
 *  optimization and lets the user fix it. Re-checks when the screen resumes. */
@Composable
private fun BatteryCard() {
    val ctx = LocalContext.current
    var exempt by remember { mutableStateOf(BatteryHelper.isExempt(ctx)) }
    val lifecycle = androidx.compose.ui.platform.LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val obs = androidx.lifecycle.LifecycleEventObserver { _, e ->
            if (e == androidx.lifecycle.Lifecycle.Event.ON_RESUME) exempt = BatteryHelper.isExempt(ctx)
        }
        lifecycle.addObserver(obs)
        onDispose { lifecycle.removeObserver(obs) }
    }
    Column(Modifier.fillMaxWidth().glass().padding(14.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Bolt, null, tint = if (exempt) good else goldInk, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text("Always-on delivery", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            Text(if (exempt) "On" else "Off", color = if (exempt) good else goldInk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
        }
        Spacer(Modifier.height(6.dp))
        Text(
            if (exempt) "Hey can receive messages in the background. No servers, no Google — just your device on the carrier."
            else "Allow Hey to run in the background so DMs and posts arrive even when it's closed. Uses very little battery.",
            color = muted, fontSize = 13.sp,
        )
        if (!exempt) {
            Spacer(Modifier.height(10.dp))
            Button(onClick = { BatteryHelper.request(ctx) }, colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)) {
                Icon(Icons.Filled.Bolt, null, Modifier.size(16.dp)); Spacer(Modifier.width(6.dp)); Text("Allow background", fontWeight = FontWeight.Bold)
            }
        }
    }
}

// ── theme toggle (dark navy ↔ light silver+gold) ─────────────────────────────

@Composable
private fun ThemeToggle() {
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE) }
    fun set(light: Boolean) { heyLight = light; prefs.edit().putBoolean("light", light).apply() }
    Row(Modifier.glass(24.dp).padding(4.dp)) {
        ThemeChip("Dark", !heyLight) { set(false) }
        ThemeChip("Light", heyLight) { set(true) }
    }
}

@Composable
private fun ThemeChip(label: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.clip(RoundedCornerShape(20.dp))
            .background(if (selected) Gold else Color.Transparent)
            .clickable { onClick() }
            .padding(18.dp, 9.dp)
    ) { Text(label, color = if (selected) Navy else muted, fontWeight = FontWeight.SemiBold, fontSize = 13.sp) }
}

// ── small shared helpers ─────────────────────────────────────────────────────

@Composable
private fun VideoTile(cid: String) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var loading by remember { mutableStateOf(false) }
    Box(
        Modifier.fillMaxWidth().height(240.dp).clip(RoundedCornerShape(14.dp))
            .background(Color.Black.copy(alpha = 0.40f))
            .clickable(enabled = !loading) {
                // Resolve the namespace to bytes, stage to cache, hand an external
                // player a FileProvider URI — no IP/gateway, no elastos:// leak.
                loading = true
                scope.launch {
                    val uri = withContext(Dispatchers.IO) {
                        runCatching {
                            val bytes = HeyApi.contentBytes(cid); if (bytes.isEmpty()) return@runCatching null
                            val dir = java.io.File(ctx.cacheDir, "media").apply { mkdirs() }
                            val f = java.io.File(dir, cid.filter { it.isLetterOrDigit() }.take(40) + ".mp4").apply { writeBytes(bytes) }
                            androidx.core.content.FileProvider.getUriForFile(ctx, ctx.packageName + ".files", f)
                        }.getOrNull()
                    }
                    loading = false
                    if (uri != null) runCatching {
                        ctx.startActivity(
                            android.content.Intent(android.content.Intent.ACTION_VIEW)
                                .setDataAndType(uri, "video/*")
                                .addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        )
                    }
                }
            },
        Alignment.Center
    ) {
        if (loading) CircularProgressIndicator(color = Color.White)
        else Icon(Icons.Filled.PlayCircle, "Play video", tint = Color.White, modifier = Modifier.size(58.dp))
    }
}

private fun relativeTime(ts: Long): String {
    if (ts <= 0L) return ""
    return android.text.format.DateUtils.getRelativeTimeSpanString(
        ts, System.currentTimeMillis(), android.text.format.DateUtils.MINUTE_IN_MILLIS
    ).toString()
}

/** Short clock time (HH:mm) shown inside chat bubbles, Telegram-style. */
private fun clockTime(ts: Long): String =
    if (ts <= 0L) "" else java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(ts))

// ── people: rows, activity (notifications), peer profile ─────────────────────

@Composable
private fun PersonRow(did: String, onClick: () -> Unit, trailing: (@Composable () -> Unit)? = null) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 4.dp).glass(14.dp).clickable { onClick() }.padding(12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(Modifier.size(34.dp).clip(RoundedCornerShape(17.dp)).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
            Text(did.removePrefix("did:key:z").take(1).uppercase(), color = Navy, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.width(10.dp))
        Text(did.removePrefix("did:key:z").take(18) + "…", color = ink, fontSize = 13.sp, modifier = Modifier.weight(1f))
        trailing?.invoke()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotificationsScreen(topPad: Dp = 12.dp, onOpenProfile: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    var followers by remember { mutableStateOf<List<Follow>>(emptyList()) }
    var followingSet by remember { mutableStateOf<Set<String>>(emptySet()) }
    var dismissed by remember { mutableStateOf(HeyApi.dismissedNotifs(ctx)) }
    var loaded by remember { mutableStateOf(false) }
    var tick by remember { mutableStateOf(0) }
    LaunchedEffect(tick) {
        while (true) {
            followers = withContext(Dispatchers.IO) { runCatching { HeyApi.followers() }.getOrDefault(emptyList()) }
            followingSet = withContext(Dispatchers.IO) { runCatching { HeyApi.following().map { it.did }.toSet() }.getOrDefault(emptySet()) }
            loaded = true
            kotlinx.coroutines.delay(3000)
        }
    }
    fun dismiss(did: String) {
        HeyApi.setNotifDismissed(ctx, did, true)
        dismissed = dismissed + did
    }
    val shown = followers.filter { it.did !in dismissed }
    Column(Modifier.fillMaxSize().padding(top = topPad)) {
        Text("Activity", color = ink, fontSize = 20.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(18.dp, 14.dp))
        if (loaded && shown.isEmpty()) {
            Column(Modifier.fillMaxSize(), Arrangement.Center, Alignment.CenterHorizontally) {
                Icon(Icons.Filled.Notifications, null, tint = muted, modifier = Modifier.size(46.dp))
                Spacer(Modifier.height(12.dp))
                Text("No activity yet", color = ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text("Share your QR (Profile) so people can follow you.", color = muted, fontSize = 13.sp)
            }
        } else {
            LazyColumn(contentPadding = PaddingValues(start = 12.dp, end = 12.dp, bottom = 96.dp)) {
                items(shown, key = { it.did }) { f ->
                    val dismissState = rememberSwipeToDismissBoxState(
                        confirmValueChange = { v ->
                            if (v != SwipeToDismissBoxValue.Settled) { dismiss(f.did); true } else false
                        }
                    )
                    SwipeToDismissBox(
                        state = dismissState,
                        modifier = Modifier.animateItem(),
                        backgroundContent = {
                            // Only reveal the red swipe-action while actually swiping (else it bleeds
                            // through the translucent row and looks like a 2nd X).
                            if (dismissState.dismissDirection != SwipeToDismissBoxValue.Settled) {
                                val align = if (dismissState.dismissDirection == SwipeToDismissBoxValue.StartToEnd) Alignment.CenterStart else Alignment.CenterEnd
                                Box(Modifier.fillMaxSize().padding(vertical = 4.dp).clip(RoundedCornerShape(14.dp)).background(Like.copy(alpha = 0.16f)).padding(horizontal = 22.dp), align) {
                                    Icon(Icons.Filled.Close, "Dismiss", tint = Like)
                                }
                            }
                        },
                    ) {
                        Row(Modifier.fillMaxWidth().padding(vertical = 4.dp).glass(14.dp).clickable { onOpenProfile(f.did) }.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.size(38.dp).clip(RoundedCornerShape(19.dp)).background(Brush.linearGradient(listOf(Gold, Gold2))), Alignment.Center) {
                                Text(f.did.removePrefix("did:key:z").take(1).uppercase(), color = Navy, fontWeight = FontWeight.Bold)
                            }
                            Spacer(Modifier.width(10.dp))
                            Column(Modifier.weight(1f)) {
                                Text(HeyApi.shortDid(f.did), color = ink, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                Text("started following you", color = muted, fontSize = 12.sp)
                            }
                            if (f.did in followingSet) {
                                Text("Following", color = muted, fontSize = 12.sp)
                            } else {
                                Button(
                                    onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.followBack(f.did) } }; tick++ } },
                                    colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy),
                                    contentPadding = PaddingValues(14.dp, 6.dp)
                                ) { Text("Follow back", fontSize = 12.sp, fontWeight = FontWeight.SemiBold) }
                            }
                            IconButton(onClick = { dismiss(f.did) }, modifier = Modifier.size(30.dp)) {
                                Icon(Icons.Filled.Close, "Dismiss", tint = muted, modifier = Modifier.size(16.dp))
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun UserProfileScreen(did: String, onBack: () -> Unit, onMessage: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var posts by remember { mutableStateOf<List<Post>>(emptyList()) }
    var followingThem by remember { mutableStateOf(false) }
    var prof by remember { mutableStateOf(Profile(did, "", "", "")) }
    var status by remember { mutableStateOf("") }
    var showTip by remember { mutableStateOf(false) }
    LaunchedEffect(did) {
        prof = withContext(Dispatchers.IO) { runCatching { HeyApi.profile(did) }.getOrDefault(Profile(did, "", "", "")) }
        posts = withContext(Dispatchers.IO) { runCatching { HeyApi.userPosts(did) }.getOrDefault(emptyList()) }
        followingThem = withContext(Dispatchers.IO) { runCatching { JSONObject(HeyApi.hey_is_following(did)).optBoolean("following") }.getOrDefault(false) }
    }
    androidx.activity.compose.BackHandler { onBack() }
    Box(Modifier.fillMaxSize().background(bg2)) {
        LazyVerticalGrid(
            columns = GridCells.Fixed(3),
            modifier = Modifier.fillMaxSize().statusBarsPadding(),
            contentPadding = PaddingValues(2.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = ink) }
                        Text("Profile", color = ink, fontWeight = FontWeight.SemiBold, fontSize = 17.sp)
                    }
                    Spacer(Modifier.height(8.dp))
                    Avatar(prof.avatar, did, 84)
                    Spacer(Modifier.height(12.dp))
                    Text(prof.nickname.ifBlank { HeyApi.shortDid(did) }, color = ink, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                    if (prof.bio.isNotBlank()) {
                        Spacer(Modifier.height(4.dp))
                        Text(prof.bio, color = muted, fontSize = 13.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center, modifier = Modifier.padding(horizontal = 16.dp))
                    }
                    Spacer(Modifier.height(4.dp))
                    Text(did, color = muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = 24.dp))
                    Spacer(Modifier.height(14.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Button(
                            onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.followBack(did) } }; followingThem = true } },
                            enabled = !followingThem,
                            colors = ButtonDefaults.buttonColors(containerColor = Gold, contentColor = Navy)
                        ) { Text(if (followingThem) "Following" else "Follow", fontWeight = FontWeight.Bold) }
                        OutlinedButton(onClick = {
                            scope.launch {
                                val err = withContext(Dispatchers.IO) { HeyApi.startChat(did) }
                                if (err == null) onMessage(did) else status = err
                            }
                        }) { Icon(Icons.Filled.Forum, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Message", color = ink) }
                        OutlinedButton(onClick = { showTip = true }) {
                            Icon(Icons.Filled.Paid, null, Modifier.size(18.dp), tint = goldInk); Spacer(Modifier.width(6.dp)); Text("Tip", color = ink)
                        }
                    }
                    if (showTip) TipSheet(did, prof.nickname.ifBlank { HeyApi.shortDid(did) }) { showTip = false }
                    if (status.isNotBlank()) { Spacer(Modifier.height(8.dp)); Text(status, color = muted, fontSize = 12.sp) }
                    Spacer(Modifier.height(16.dp))
                    Text("Posts (${posts.size})", color = ink, fontWeight = FontWeight.SemiBold, modifier = Modifier.align(Alignment.Start))
                    Spacer(Modifier.height(6.dp))
                }
            }
            gridItems(posts, key = { it.id }) { p ->
                val cid = p.media.firstOrNull { it.type != "video" }?.cid
                Box(Modifier.padding(2.dp).aspectRatio(1f).clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.25f)), Alignment.Center) {
                    if (cid != null) {
                        AsyncImage(model = HeyApi.mediaUri(cid), contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                    } else if (p.media.any { it.type == "video" }) {
                        Icon(Icons.Filled.PlayCircle, null, tint = Color.White, modifier = Modifier.size(28.dp))
                    } else {
                        Text(p.caption.take(18), color = muted, fontSize = 10.sp, modifier = Modifier.padding(4.dp))
                    }
                }
            }
        }
    }
}
