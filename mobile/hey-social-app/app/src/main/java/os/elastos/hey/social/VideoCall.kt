package os.elastos.hey.social

import android.content.Context
import android.view.Surface
import androidx.camera.view.PreviewView
import androidx.lifecycle.LifecycleOwner
import kotlin.concurrent.thread

/**
 * Video-call media engine — the visual sibling of [VoiceAudio]. CameraX → H.264
 * hardware encode → the Rust `hey/video/1` plane ([HeyApi.videoSendFrame]); and
 * [HeyApi.videoRecvFrame] → H.264 decode → the remote [Surface]. DIRECT-ONLY
 * (the Rust `videoStart` refuses a relay peer). Voice runs independently — a video
 * call is a voice call with this lane also up; stopping video leaves audio intact.
 *
 * Wire framing INSIDE the opaque transport frame: `[1B flags: bit0=keyframe][8B
 * ptsUs LE][H.264 payload]` (SPS/PPS are prepended to keyframes by the encoder, so
 * the decoder can start/recover mid-stream).
 *
 * Adaptive bitrate: targets 1080p but watches the send-drop counter
 * ([HeyApi.videoDropped]) — if the network falls behind it cuts the bitrate + re-keys
 * (no lag), then ramps back toward 1080p when the link is clean again.
 */
object VideoCall {
    private const val FPS = 24
    private const val BITRATE_MAX = 4_000_000   // 1080p ceiling
    private const val BITRATE_MIN = 600_000     // floor on a poor link
    private const val BITRATE_START = 2_800_000

    @Volatile private var running = false
    private var encoder: VideoEncoder? = null
    // MediaCodec is NOT thread-safe: the decoder is fed on the "video-recv" thread but
    // released/recreated on the main thread (setRemoteSurface/stop). A release during an
    // in-flight feed() is native UB (SIGSEGV) that runCatching can't catch — so ALL
    // decoder access is serialized through decoderLock.
    @Volatile private var decoder: VideoDecoder? = null
    private val decoderLock = Any()
    private var recvThread: Thread? = null
    private var capture: VideoCapture? = null
    @Volatile private var bitrate = BITRATE_START

    /** Visible dimensions of the REMOTE video (from its decoder), for aspect-correct
     *  rendering. 0 until the first keyframe is decoded. */
    @Volatile var remoteW = 0
        private set
    @Volatile var remoteH = 0
        private set

    // Diagnostics: frame counters so logcat shows which direction is flowing.
    @Volatile private var sentFrames = 0L
    @Volatile private var recvFrames = 0L

    /** A fresh decoder MUST be fed a keyframe FIRST (it carries SPS/PPS). Until we've
     *  seen one, inbound P-frames are dropped — feeding them to an unconfigured decoder
     *  wedges it (no output ever → black remote). Reset whenever the decoder is (re)created. */
    @Volatile private var sawKeyframe = false

    /**
     * Start a 1:1 video session. `remoteSurface` renders the peer; `previewView` is
     * the local self-view; `peerDid` resolves the carrier ticket; needs CAMERA granted
     * to send (a receive-only call still shows the peer). Idempotent.
     */
    fun start(
        ctx: Context,
        lifecycleOwner: LifecycleOwner,
        peerDid: String,
        camGranted: Boolean,
        previewView: PreviewView?,
    ) {
        if (running) return
        running = true
        bitrate = BITRATE_START
        remoteW = 0; remoteH = 0; sentFrames = 0; recvFrames = 0; sawKeyframe = false
        android.util.Log.i("video", "VideoCall.start peer=…${peerDid.takeLast(6)} camGranted=$camGranted")

        // Open the video transport IMMEDIATELY (don't wait on the render surface) so
        // the link forms exactly like voice — the decoder attaches later via
        // setRemoteSurface(). Direct-only enforced in Rust.
        thread(name = "video-connect") {
            val ticket = runCatching { HeyApi.peerTicket(peerDid) }.getOrDefault("")
            if (ticket.isNotEmpty()) HeyApi.videoStart(ticket)
        }

        // Receive ferry: drains frames; feeds the decoder once a surface is attached
        // (frames before that are dropped — the next keyframe, ≤2s, resyncs).
        recvThread = thread(name = "video-recv") {
            while (running) {
                val wire = HeyApi.videoRecvFrame()
                if (wire.size < 9) {
                    Thread.sleep(4)
                    continue
                }
                // Gate on the first keyframe so a fresh/just-recreated decoder configures
                // from SPS/PPS instead of wedging on P-frames (the black-remote bug).
                val isKey = (wire[0].toInt() and 1) == 1
                if (!sawKeyframe) {
                    if (!isKey) continue
                    sawKeyframe = true
                }
                val pts = leLong(wire, 1)
                val h264 = wire.copyOfRange(9, wire.size)
                recvFrames++
                synchronized(decoderLock) { runCatching { decoder?.feed(h264, pts) } }
            }
        }

        // Encoder + camera capture (sending). Skipped if the camera isn't granted.
        if (camGranted) {
            val enc = runCatching {
                VideoEncoder(bitrate, FPS) { frame, key ->
                    val wire = ByteArray(9 + frame.size)
                    wire[0] = if (key) 1 else 0
                    putLeLong(wire, 1, System.nanoTime() / 1000)
                    System.arraycopy(frame, 0, wire, 9, frame.size)
                    sentFrames++
                    HeyApi.videoSendFrame(wire)
                }
            }.getOrNull()
            encoder = enc
            if (enc != null) {
                capture = runCatching {
                    VideoCapture(ctx, previewView) { image, pts, rot ->
                        enc.encode(image, pts, rot)
                    }.also { it.start() }
                }.getOrNull()
            }
        }

        startAdaptive()
    }

    private fun startAdaptive() {
        thread(name = "video-adapt") {
            var lastDropped = HeyApi.videoDropped()
            var clean = 0
            while (running) {
                Thread.sleep(2000)
                if (!running) break
                val now = HeyApi.videoDropped()
                val delta = now - lastDropped
                lastDropped = now
                // Directionality diagnostic: sent>0 = we encode+send; recv>0 = peer
                // sends to us; peers = live link; decoder = our render surface is up.
                android.util.Log.i(
                    "video",
                    "stat peers=${runCatching { HeyApi.videoPeers() }.getOrDefault(-1)} " +
                        "sent=$sentFrames recv=$recvFrames dropped=$now " +
                        "encoder=${encoder != null} decoder=${decoder != null} remote=${remoteW}x${remoteH}",
                )
                val enc = encoder ?: continue
                when {
                    delta > 3 -> {
                        // Network behind → cut bitrate + re-key for a clean restart.
                        bitrate = maxOf(BITRATE_MIN, bitrate * 6 / 10)
                        runCatching { enc.setBitrate(bitrate); enc.requestKeyframe() }
                        clean = 0
                    }
                    delta == 0L -> {
                        clean++
                        if (clean >= 3 && bitrate < BITRATE_MAX) {
                            bitrate = minOf(BITRATE_MAX, bitrate + 400_000)
                            runCatching { enc.setBitrate(bitrate) }
                            clean = 0
                        }
                    }
                    else -> clean = 0
                }
            }
        }
    }

    /** Attach (or replace) the remote render surface. The decoder is created HERE,
     *  decoupled from the transport (which already started in [start]) so the link
     *  forms immediately even if the surface lags. Frames before this are dropped;
     *  the next keyframe (≤2s) resyncs. */
    fun setRemoteSurface(surface: Surface) {
        if (!running) return
        synchronized(decoderLock) {
            runCatching { decoder?.release() }
            decoder = runCatching {
                VideoDecoder(
                    surface,
                    onSize = { w, h -> remoteW = w; remoteH = h },
                    onReset = { sawKeyframe = false }, // after a decoder reset, wait for a keyframe
                )
            }.getOrNull()
            sawKeyframe = false // the new decoder must (re)start on a keyframe
        }
        android.util.Log.i("video", "setRemoteSurface decoder=${decoder != null}")
    }

    /** Remote video aspect (w/h), or 0 until the first remote keyframe decodes. */
    fun remoteAspect(): Float {
        val w = remoteW; val h = remoteH
        return if (w > 0 && h > 0) w.toFloat() / h.toFloat() else 0f
    }

    /** Camera-off toggle: stop emitting frames (the lane + the peer's video stay up). */
    fun setVideoMuted(muted: Boolean) {
        runCatching { HeyApi.videoSetPaused(muted) }
    }

    fun flipCamera() {
        runCatching { capture?.flip() }
    }

    /** Live video link up? (false while still dialing — drives "connecting video…"). */
    fun connected(): Boolean = runCatching { HeyApi.videoPeers() > 0 }.getOrDefault(false)

    fun stop() {
        if (!running) return
        running = false
        // Join the recv thread first so no feed() is in flight, THEN release the decoder
        // under the lock — a release racing an in-flight feed() is a native crash.
        runCatching { recvThread?.join(500L) }; recvThread = null
        runCatching { capture?.release() }; capture = null
        runCatching { encoder?.release() }; encoder = null
        synchronized(decoderLock) {
            runCatching { decoder?.release() }; decoder = null
        }
        runCatching { HeyApi.videoStop() }
    }

    private fun leLong(b: ByteArray, off: Int): Long {
        var v = 0L
        for (i in 0 until 8) v = v or ((b[off + i].toLong() and 0xff) shl (8 * i))
        return v
    }

    private fun putLeLong(b: ByteArray, off: Int, v: Long) {
        for (i in 0 until 8) b[off + i] = ((v shr (8 * i)) and 0xff).toByte()
    }
}
