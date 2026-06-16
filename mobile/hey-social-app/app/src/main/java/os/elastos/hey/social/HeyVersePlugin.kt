package os.elastos.hey.social

import org.godotengine.godot.Godot
import org.godotengine.godot.plugin.GodotPlugin
import org.godotengine.godot.plugin.UsedByGodot
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * Hey Verse <-> Hey runtime bridge.
 *
 * Verse traffic rides the runtime's dedicated VERSE LANE (hey_verse_send /
 * hey_verse_poll): sealed + ratcheted exactly like a DM on the wire, but the
 * receiver diverts it into an in-memory inbox — it never appears in
 * conversations, never counts as unread, never notifies, and never competes
 * with call signaling. [startLane] runs the single app-wide drain thread:
 * signals route to the live plugin instance, and INVITES surface app-wide as
 * an Accept/Decline popup (even when Verse is closed).
 *
 * Session model is LIVE-ONLY: presence = receiving signals; a peer silent for
 * >12s (or sending "bye") drops, and a fresh invite is needed to rejoin.
 *
 * Protocol (payload JSON): k = inv | ok | mv | chat | bye
 *   inv: {name, w, ts}   ok: {name, w}   mv: {x, z, yw, m, w}   chat: {tx}
 * `w` = world zone ("home" | "city") so the acceptor lands in the inviter's
 * world and ghosts never render across worlds.
 */
class HeyVersePlugin(godot: Godot) : GodotPlugin(godot) {

    companion object {
        @Volatile private var instance: HeyVersePlugin? = null

        /** True once the game scene reported in — drives the loading overlay. */
        @JvmStatic @Volatile var gameReadyFlag = false

        /** Engine heartbeat: the game polls every frame, so a stale stamp while the
         *  activity is RESUMED means the render/script loop is WEDGED (the GL
         *  black-screen state) — the watchdog in MainActivity acts on it. */
        @JvmStatic @Volatile var lastPollAt = 0L

        /** UI commands from the Compose dock sheets, drained by the game. */
        private val uiQueue = java.util.concurrent.ConcurrentLinkedQueue<String>()

        /** Game -> Compose sheet requests (tap Sash → FAQ). */
        @Volatile private var sheetRequest: String? = null

        /** A pending verse invite for the Accept/Decline popup: (did, name, world). */
        @JvmStatic @Volatile var pendingInvite: Triple<String, String, String>? = null

        /** Accepted invite waiting for the game to join: consumed by pollJson. */
        @Volatile internal var acceptedInvite: Triple<String, String, String>? = null
        // Drain fast (realtime) only while a verse session is live; idle otherwise
        // so the always-on daemon does not 20Hz-poll the JNI for nothing.
        @JvmStatic @Volatile var verseActive = false

        private var lane: Thread? = null

        /** The single app-wide verse-lane drain. Start once at app boot. */
        @JvmStatic fun startLane() {
            if (lane?.isAlive == true) return
            lane = Thread {
                while (true) {
                    // the REALTIME lane first: movement arrives as QUIC datagrams
                    runCatching {
                        for ((from, p) in HeyApi.verseRtPoll()) instance?.onSignal(from, p)
                    }
                    runCatching {
                        for ((from, p) in HeyApi.versePoll()) {
                            if (p.optString("k") == "inv") {
                                // fresh invites only — a stale one joins nobody
                                val ts = p.optLong("ts", 0L)
                                if (ts == 0L || System.currentTimeMillis() - ts < 90_000) {
                                    pendingInvite = Triple(
                                        from,
                                        p.optString("name").ifBlank { HeyApi.shortDid(from) },
                                        p.optString("w", "home"),
                                    )
                                }
                            } else {
                                instance?.onSignal(from, p)
                            }
                        }
                    }
                    try { Thread.sleep(if (verseActive) 45L else 200L) } catch (_: InterruptedException) { return@Thread }
                }
            }.apply { isDaemon = true; name = "verse-lane" }.also { it.start() }
        }

        /** The user tapped Join on the invite popup. */
        @JvmStatic fun acceptInvite() {
            acceptedInvite = pendingInvite
            pendingInvite = null
        }

        @JvmStatic fun declineInvite() {
            pendingInvite = null
        }

        /** Compose -> game: queue a command ("hat", "preset_night", "save"…). */
        @JvmStatic fun postUi(cmd: String) {
            uiQueue.add(cmd)
        }

        /** Compose -> network: invite a contact directly (no game roundtrip). */
        @JvmStatic fun inviteContact(did: String) {
            instance?.invite(did)
        }

        @JvmStatic fun takeSheetRequest(): String? {
            val r = sheetRequest
            sheetRequest = null
            return r
        }

        internal fun drainUi(): List<String> {
            val out = ArrayList<String>()
            while (true) out.add(uiQueue.poll() ?: break)
            return out
        }
    }

    private class Peer(
        var name: String,
        var x: Float = 0f,
        var z: Float = 2f,
        var yaw: Float = 0f,
        var moving: Boolean = false,
        var sitting: Boolean = false,
        var zone: String = "home",
        var last: Long = System.currentTimeMillis(),
    )

    private val peers = HashMap<String, Peer>()           // present in the shared session
    private val chats = ArrayList<Pair<String, String>>() // inbound (did, text)
    private val ended = ArrayList<String>()               // sessions that closed
    private val sender = Executors.newSingleThreadExecutor { r ->
        Thread(r, "verse-send").apply { isDaemon = true }
    }
    private var lastMove = 0L
    @Volatile private var myName = "me"
    @Volatile private var myDid = ""
    @Volatile private var myZone = "home"
    private var gossipZone = ""   // world topic the ephemeral gossip lane is joined to
    @Volatile private var joinWorld: String? = null

    init {
        instance = this
        sender.execute {
            runCatching { myDid = HeyApi.whoami().optString("did", "") }
            runCatching {
                val n = HeyApi.profile().nickname
                if (n.isNotBlank()) myName = n
            }
        }
    }

    override fun getPluginName() = "HeyVerse"

    // ── inbound (from the lane thread) ────────────────────────────────────────

    internal fun onSignal(from: String, p: JSONObject) {
        val now = System.currentTimeMillis()
        when (p.optString("k")) {
            "ok" -> {
                HeyApi.verseRtJoin(from)   // they accepted: bring the fast lane up
                val name = p.optString("name").ifBlank { HeyApi.shortDid(from) }
                synchronized(peers) {
                    peers.getOrPut(from) { Peer(name) }.also {
                        it.name = name
                        it.zone = p.optString("w", it.zone)
                        it.last = now
                    }
                }
                gossipZone = myZone
                HeyApi.verseGossipJoin(myZone, presentDids())   // ephemeral presence topic
                verseActive = true
            }
            "mv" -> synchronized(peers) {
                val peer = peers.getOrPut(from) { Peer(HeyApi.shortDid(from)) }
                peer.x = p.optDouble("x", 0.0).toFloat()
                peer.z = p.optDouble("z", 2.0).toFloat()
                peer.yaw = p.optDouble("yw", 0.0).toFloat()
                peer.moving = p.optBoolean("m", false)
                peer.sitting = p.optBoolean("s", false)
                peer.zone = p.optString("w", peer.zone)
                peer.last = now
            }
            "chat" -> {
                val tx = p.optString("tx")
                if (tx.isNotBlank()) {
                    synchronized(peers) { peers[from]?.last = now }
                    synchronized(chats) { chats.add(from to tx) }
                }
            }
            "bye" -> synchronized(peers) {
                if (peers.remove(from) != null) synchronized(ended) { ended.add(from) }
                if (peers.isEmpty()) { HeyApi.verseRtReset(); HeyApi.verseGossipReset(); gossipZone = ""; verseActive = false }
            }
        }
    }

    private fun verse(k: String): JSONObject =
        JSONObject().put("k", k)

    private fun send(did: String, payload: JSONObject) {
        sender.execute { runCatching { HeyApi.verseSend(did, payload) } }
    }

    private fun presentDids(): List<String> = synchronized(peers) { peers.keys.toList() }

    // ── surface for GDScript (called on the engine thread) ───────────────────

    @UsedByGodot
    fun localDid(): String = myDid

    @UsedByGodot
    fun localName(): String = myName

    /** The game scene is up — hide the boot overlay. */
    @UsedByGodot
    fun gameReady() {
        gameReadyFlag = true
        lastPollAt = System.currentTimeMillis()
    }

    /** The game asks the app to open a popup sheet ("sash_faq", …). */
    @UsedByGodot
    fun openSheet(name: String) {
        sheetRequest = name
    }

    // ── DDRM encrypted 3D assets (local-first, no chain) ─────────────────────
    /** Encrypt+store a base64 `.glb` with content key `ck` → its cid ("" on error).
     *  GDScript: read `res://…glb` bytes → `Marshalls.raw_to_base64` → packDdrm. */
    @UsedByGodot
    fun packDdrm(glbB64: String, ck: String): String = HeyApi.ddrmPack(glbB64, ck) ?: ""

    /** Fetch+decrypt a `.ddrm` by cid → base64 `.glb` ("" on error). GDScript:
     *  `Marshalls.base64_to_raw` → `GLTFDocument.append_from_buffer`. Decrypts ON-DEVICE. */
    @UsedByGodot
    fun loadDdrm(cid: String, ck: String): String = HeyApi.ddrmLoadB64(cid, ck) ?: ""

    /** Your real Hey contacts (1:1 only) as [{did, name}] — the invite picker. */
    @UsedByGodot
    fun contactsJson(): String {
        val arr = JSONArray()
        runCatching {
            for (c in HeyApi.chats()) if (!c.isGroup) {
                arr.put(JSONObject().put("did", c.id).put("name", c.name))
            }
        }
        return arr.toString()
    }

    @UsedByGodot
    fun invite(did: String) {
        send(
            did,
            verse("inv").put("name", myName).put("w", myZone)
                .put("ts", System.currentTimeMillis()),
        )
    }

    @UsedByGodot
    fun sendMove(x: Float, z: Float, yaw: Float, moving: Boolean, sitting: Boolean = false) {
        myZone = if (z < -100f) "city" else "home"
        val now = System.currentTimeMillis()
        if (now - lastMove < 55) return
        lastMove = now
        // Walked into the other world (home/city are separate presence topics):
        // re-point the gossip lane. Only once a session is live (gossipZone set).
        if (gossipZone.isNotEmpty() && myZone != gossipZone) {
            gossipZone = myZone
            HeyApi.verseGossipJoin(myZone, presentDids())
        }
        val payload = verse("mv").put("x", x.toDouble()).put("z", z.toDouble())
            .put("yw", yaw.toDouble()).put("m", moving).put("s", sitting).put("w", myZone)
        // Movement NEVER touches the DM/ratchet lane. Two EPHEMERAL lanes carry
        // it, both no-PQ and no-disk, both depositing into the same inbox
        // (the receiver is last-write-wins on position):
        //  • verse_rt     — direct QUIC datagrams, lowest latency when a link is up;
        //  • verse_gossip — raw unsealed gossip on the world topic, reaching peers
        //    that don't (yet) have a direct datagram link.
        HeyApi.verseRtSend(payload.toString())
        HeyApi.verseGossipSend(payload.toString())
    }

    @UsedByGodot
    fun sendChat(text: String) {
        val payload = verse("chat").put("tx", text)
        for (did in presentDids()) send(did, payload)
    }

    /** Drain: {peers:{did:{x,z,yw,m,w,name}}, chats, ended, ui, me, join?} */
    @UsedByGodot
    fun pollJson(): String {
        lastPollAt = System.currentTimeMillis()
        // an accepted invite: greet the inviter and tell the game which world
        acceptedInvite?.let { (did, name, world) ->
            acceptedInvite = null
            synchronized(peers) {
                peers.getOrPut(did) { Peer(name) }.also {
                    it.zone = world
                    it.last = System.currentTimeMillis()
                }
            }
            send(did, verse("ok").put("name", myName).put("w", world))
            HeyApi.verseRtJoin(did)   // we accepted: fast lane to the inviter
            gossipZone = world
            HeyApi.verseGossipJoin(world, presentDids())   // + ephemeral presence topic
            verseActive = true
            joinWorld = world
        }
        val now = System.currentTimeMillis()
        val o = JSONObject()
        val po = JSONObject()
        synchronized(peers) {
            val it = peers.entries.iterator()
            while (it.hasNext()) {
                val e = it.next()
                if (now - e.value.last > 12_000) {        // live-only: silence = gone
                    synchronized(ended) { ended.add(e.key) }
                    it.remove()
                    if (peers.isEmpty()) { HeyApi.verseRtReset(); HeyApi.verseGossipReset(); gossipZone = ""; verseActive = false }   // session emptied: drop both ephemeral lanes
                    continue
                }
                po.put(e.key, JSONObject()
                    .put("x", e.value.x.toDouble()).put("z", e.value.z.toDouble())
                    .put("yw", e.value.yaw.toDouble()).put("m", e.value.moving)
                    .put("s", e.value.sitting)
                    .put("w", e.value.zone)
                    .put("name", e.value.name))
            }
        }
        o.put("peers", po)
        val ca = JSONArray()
        synchronized(chats) {
            for ((d, t) in chats) ca.put(JSONArray().put(d).put(t))
            chats.clear()
        }
        o.put("chats", ca)
        val ea = JSONArray()
        synchronized(ended) {
            for (d in ended) ea.put(d)
            ended.clear()
        }
        o.put("ended", ea)
        val ua = JSONArray()
        for (c in drainUi()) ua.put(c)
        o.put("ui", ua)
        o.put("me", JSONObject().put("did", myDid).put("name", myName))
        joinWorld?.let { o.put("join", it); joinWorld = null }
        return o.toString()
    }
}
