package os.elastos.hey.social

import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * 1:1 voice-call state machine + signaling driver.
 *
 * Stage 1 = call SETUP only (ring / accept / decline / hang-up). The actual audio is Stage 2
 * (an iroh voice ALPN + Opus over QUIC datagrams). Signals ride the existing E2E DM channel via
 * [HeyApi.callSend] / [HeyApi.callPoll], so they're encrypted + serverless like everything else.
 *
 * Process-scoped singleton so a call survives screen changes; the UI just renders [state].
 */
object CallManager {
    /** A participant in a live group call (drawn in the overlay; tickets drive the audio mesh). */
    data class GroupParticipant(val did: String, val name: String, val ticket: String, val mine: Boolean)

    sealed interface State {
        object Idle : State
        data class Outgoing(val peer: String, val name: String, val callId: String, val video: Boolean = false) : State
        data class Incoming(val peer: String, val name: String, val callId: String, val video: Boolean = false) : State
        data class Active(val peer: String, val name: String, val callId: String, val sinceElapsed: Long, val isCaller: Boolean, val video: Boolean = false) : State
        /** A group call I'm in: a full mesh, announced/joined via the group thread. */
        data class GroupActive(
            val gid: String,
            val callId: String,
            val title: String,
            val participants: List<GroupParticipant>,
            val sinceElapsed: Long,
        ) : State
    }

    var state by mutableStateOf<State>(State.Idle)
        private set

    private val scope = CoroutineScope(Dispatchers.Main.immediate + SupervisorJob())
    private var pollJob: Job? = null
    private var groupJob: Job? = null
    // Suppress a re-ring from a just-ended call whose "offer" is still inside the 2-min poll window.
    @Volatile private var lastEndedCallId: String? = null

    private fun sig(type: String, callId: String, name: String? = null, video: Boolean = false) = JSONObject()
        .put("type", type)
        .put("call_id", callId)
        .apply {
            if (name != null) put("name", name)
            if (video) put("video", true)
        }

    private fun currentPeerCall(): Pair<String, String>? = when (val s = state) {
        is State.Outgoing -> s.peer to s.callId
        is State.Incoming -> s.peer to s.callId
        is State.Active -> s.peer to s.callId
        is State.GroupActive -> null
        State.Idle -> null
    }

    /** Notifies the system when a call rings / stops ringing, so a backgrounded or
     *  locked receiver gets a proper incoming-CALL notification (full-screen, ring,
     *  Answer/Decline). Set by RuntimeService (it has the Context). */
    @Volatile var onIncomingCall: ((peer: String, name: String, callId: String, video: Boolean) -> Unit)? = null
    @Volatile var onCallEnded: (() -> Unit)? = null

    /** Place a call to a 1:1 contact. `name` = the contact's display name (shown on both ends). */
    fun startCall(did: String, name: String, video: Boolean = false) {
        if (state != State.Idle || did.isBlank()) return
        val callId = java.util.UUID.randomUUID().toString()
        state = State.Outgoing(did, name, callId, video)
        scope.launch { withContext(Dispatchers.IO) { HeyApi.callSend(did, sig("offer", callId, video = video)) } }
    }

    fun accept() {
        val s = state as? State.Incoming ?: return
        onCallEnded?.invoke() // dismiss the ringing notification
        state = State.Active(s.peer, s.name, s.callId, SystemClock.elapsedRealtime(), isCaller = false, video = s.video)
        scope.launch { withContext(Dispatchers.IO) { HeyApi.callSend(s.peer, sig("accept", s.callId, video = s.video)) } }
    }

    /** Mid-call: drop video but keep audio (the overlay's transport watcher calls this
     *  if the path degrades to relay). No signal needed — the peer just sees frames stop. */
    fun demoteToVoice() {
        val s = state as? State.Active ?: return
        if (!s.video) return
        state = s.copy(video = false)
    }

    fun decline() {
        val s = state as? State.Incoming ?: return
        endLocal(s.callId)
        scope.launch { withContext(Dispatchers.IO) { HeyApi.callSend(s.peer, sig("decline", s.callId)) } }
    }

    /** Cancel an outgoing call OR hang up an active one. */
    fun hangup() {
        val (peer, callId) = currentPeerCall() ?: return
        endLocal(callId)
        scope.launch { withContext(Dispatchers.IO) { HeyApi.callSend(peer, sig("end", callId)) } }
    }

    private fun endLocal(callId: String?) {
        if (callId != null) lastEndedCallId = callId
        onCallEnded?.invoke() // dismiss any ringing notification
        state = State.Idle
    }

    // ── group calls (mesh) ────────────────────────────────────────────────────
    /** Start a group call: announce it on the group thread, open the audio mesh, drive the roster. */
    fun startGroupCall(gid: String, title: String) {
        if (state != State.Idle || gid.isBlank()) return
        scope.launch {
            val r = withContext(Dispatchers.IO) { runCatching { HeyApi.groupCallStart(gid) }.getOrNull() }
            val callId = r?.optString("call_id").orEmpty()
            if (callId.isBlank()) return@launch
            HeyApi.voiceGroupStart()
            state = State.GroupActive(gid, callId, title, emptyList(), SystemClock.elapsedRealtime())
            runGroupLoop(gid, callId)
        }
    }

    /** Join an in-progress group call (tapped from the group thread's call card). */
    fun joinGroupCall(gid: String, callId: String, title: String) {
        if (state != State.Idle || callId.isBlank()) return
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { HeyApi.groupCallSignal(gid, callId, "join") } }
            HeyApi.voiceGroupStart()
            state = State.GroupActive(gid, callId, title, emptyList(), SystemClock.elapsedRealtime())
            runGroupLoop(gid, callId)
        }
    }

    /** Leave the group call I'm in (audio is torn down by the overlay's onDispose). */
    fun hangupGroup() {
        val s = state as? State.GroupActive ?: return
        groupJob?.cancel()
        state = State.Idle
        scope.launch { withContext(Dispatchers.IO) { runCatching { HeyApi.groupCallSignal(s.gid, s.callId, "leave") } } }
    }

    /** Poll the group-call roster ~1.5s: reconcile the audio mesh + update participants; heartbeat ~45s. */
    private fun runGroupLoop(gid: String, callId: String) {
        groupJob?.cancel()
        groupJob = scope.launch {
            var beats = 0
            while (true) {
                val cur = state
                if (cur !is State.GroupActive || cur.callId != callId) break
                val r = withContext(Dispatchers.IO) { runCatching { HeyApi.groupCallRoster(gid, callId) }.getOrNull() }
                if (r != null) {
                    val parts = ArrayList<GroupParticipant>()
                    r.optJSONArray("participants")?.let { arr ->
                        for (i in 0 until arr.length()) {
                            val o = arr.getJSONObject(i)
                            parts.add(GroupParticipant(o.optString("did"), o.optString("name"), o.optString("ticket"), o.optBoolean("mine")))
                        }
                    }
                    val tickets = parts.filter { !it.mine && it.ticket.isNotEmpty() }.map { it.ticket }
                    withContext(Dispatchers.IO) { runCatching { HeyApi.voiceSync(tickets) } }
                    val s2 = state
                    if (s2 is State.GroupActive && s2.callId == callId) state = s2.copy(participants = parts)
                    if (r.optBoolean("ended", false)) { hangupGroup(); break }
                }
                beats++
                if (beats % 30 == 0) withContext(Dispatchers.IO) { runCatching { HeyApi.groupCallSignal(gid, callId, "join") } }
                delay(1500)
            }
        }
    }

    /** Start the signal poll loop. Call once from the app root; safe to call repeatedly. */
    fun startPolling() {
        if (pollJob?.isActive == true) return
        android.util.Log.i("heycall", "startPolling: launching poll loop")
        pollJob = scope.launch {
            var beats = 0
            while (true) {
                val signals = withContext(Dispatchers.IO) {
                    runCatching { HeyApi.callPoll() }.getOrDefault(emptyList())
                }
                if (signals.isNotEmpty()) android.util.Log.i("heycall", "callPoll -> ${signals.size} signal(s)")
                if (beats++ % 20 == 0) android.util.Log.i("heycall", "poll alive (beat=$beats)")
                // (Verse traffic rides its own runtime lane now — this loop is
                // calls-only again and can't be drowned by movement frames.)
                for (sg in signals) handle(sg)
                delay(1000)
            }
        }
    }

    private suspend fun handle(sg: CallSignal) {
        when (sg.type) {
            "offer" -> {
                android.util.Log.i("heycall", "OFFER from=…${sg.from.takeLast(6)} video=${sg.payload.optBoolean("video", false)} state=$state")
                if (sg.callId == lastEndedCallId) return
                if (state == State.Idle) {
                    val name = sg.payload.optString("name").ifBlank {
                        withContext(Dispatchers.IO) {
                            runCatching { HeyApi.chats().firstOrNull { it.id == sg.from }?.name }.getOrNull()
                        } ?: HeyApi.shortDid(sg.from)
                    }
                    val video = sg.payload.optBoolean("video", false)
                    state = State.Incoming(sg.from, name, sg.callId, video)
                    android.util.Log.i("heycall", "Incoming set; firing onIncomingCall (hook=${onIncomingCall != null})")
                    onIncomingCall?.invoke(sg.from, name, sg.callId, video) // ring even if backgrounded
                } else {
                    // Busy — auto-decline so the caller isn't left ringing forever.
                    withContext(Dispatchers.IO) { HeyApi.callSend(sg.from, sig("decline", sg.callId)) }
                }
            }
            "accept" -> {
                val s = state
                if (s is State.Outgoing && s.callId == sg.callId) {
                    state = State.Active(s.peer, s.name, s.callId, SystemClock.elapsedRealtime(), isCaller = true, video = s.video || sg.payload.optBoolean("video", false))
                }
            }
            "decline", "end" -> {
                val cur = currentPeerCall()
                if (cur != null && cur.second == sg.callId) endLocal(sg.callId)
            }
        }
    }
}
