package os.elastos.hey.social

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import kotlin.concurrent.thread

/**
 * Voice-call audio engine (Stage 2). 8 kHz mono PCM ↔ the Rust voice module (which μ-law-encodes and
 * ships it over QUIC datagrams on the carrier's voice ALPN). Capture comes from AudioRecord with the
 * VOICE_COMMUNICATION source so the platform engages hardware **echo cancellation / noise suppression
 * / auto-gain**; playback goes to an AudioTrack on the voice-communication path. The transport
 * (voiceStart/voiceStop) is owned here too. Started/stopped by the in-call UI (CallOverlay).
 */
object VoiceAudio {
    private const val RATE = 8000
    private const val FRAME_BYTES = 320 // 20 ms @ 8 kHz mono 16-bit = 160 samples = 320 bytes

    @Volatile private var running = false
    @Volatile private var micOn = false
    private var capture: Thread? = null
    private var playback: Thread? = null

    /** Start transport + playback (+ capture if the mic is granted). Idempotent. */
    fun start(ctx: Context, peerDid: String, isCaller: Boolean, micGranted: Boolean) {
        if (running) return
        running = true
        micOn = micGranted
        val app = ctx.applicationContext
        runCatching {
            (app.getSystemService(Context.AUDIO_SERVICE) as AudioManager).mode = AudioManager.MODE_IN_COMMUNICATION
        }
        // Resolve the peer's ticket + open the voice connection off the main thread.
        thread(name = "voice-connect") {
            val ticket = runCatching { HeyApi.peerTicket(peerDid) }.getOrDefault("")
            if (ticket.isNotEmpty()) HeyApi.voiceStart(ticket, isCaller)
        }
        startPlayback()
        if (micGranted) startCapture()
    }

    /**
     * Start the audio engine for a GROUP call: same capture/playback, but the mesh transport is
     * opened + reconciled by [CallManager] (voiceGroupStart + voiceSync), so there's no single peer
     * ticket to resolve here — capture broadcasts to every peer, playback pulls the mixed stream.
     */
    fun startGroup(ctx: Context, micGranted: Boolean) {
        if (running) return
        running = true
        micOn = micGranted
        val app = ctx.applicationContext
        runCatching {
            (app.getSystemService(Context.AUDIO_SERVICE) as AudioManager).mode = AudioManager.MODE_IN_COMMUNICATION
        }
        startPlayback()
        if (micGranted) startCapture()
    }

    /** Mute/unmute mid-call. Stops sending audio (and the capture thread) when off. */
    fun setMic(ctx: Context, on: Boolean) {
        micOn = on
        HeyApi.voiceSetMuted(!on)
        if (on && running && capture == null) startCapture()
    }

    /** Route playback to the speakerphone (on) or the earpiece (off). */
    fun setSpeaker(ctx: Context, on: Boolean) {
        runCatching {
            (ctx.applicationContext.getSystemService(Context.AUDIO_SERVICE) as AudioManager).isSpeakerphoneOn = on
        }
    }

    private fun startCapture() {
        capture = thread(name = "voice-capture") {
            val min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            val rec = try {
                AudioRecord(
                    MediaRecorder.AudioSource.VOICE_COMMUNICATION,
                    RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT,
                    maxOf(min, FRAME_BYTES * 4),
                )
            } catch (e: Throwable) {
                return@thread // no RECORD_AUDIO permission, or device refused the source
            }
            if (rec.state != AudioRecord.STATE_INITIALIZED) {
                runCatching { rec.release() }; return@thread
            }
            val sid = rec.audioSessionId
            runCatching { if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(sid)?.enabled = true }
            runCatching { if (NoiseSuppressor.isAvailable()) NoiseSuppressor.create(sid)?.enabled = true }
            runCatching { if (AutomaticGainControl.isAvailable()) AutomaticGainControl.create(sid)?.enabled = true }
            runCatching { rec.startRecording() }
            val buf = ByteArray(FRAME_BYTES)
            while (running && micOn) {
                val n = rec.read(buf, 0, buf.size)
                if (n > 0) HeyApi.voiceSend(if (n == buf.size) buf else buf.copyOf(n))
            }
            runCatching { rec.stop() }; runCatching { rec.release() }
            capture = null
        }
    }

    private fun startPlayback() {
        playback = thread(name = "voice-playback") {
            val min = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
            val track = AudioTrack.Builder()
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build()
                )
                .setAudioFormat(
                    AudioFormat.Builder()
                        .setSampleRate(RATE)
                        .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .build()
                )
                .setBufferSizeInBytes(maxOf(min, FRAME_BYTES * 4))
                .build()
            runCatching { track.play() }
            while (running) {
                val pcm = HeyApi.voiceRecv(FRAME_BYTES * 2) // up to ~40 ms
                if (pcm.isNotEmpty()) track.write(pcm, 0, pcm.size)
                else runCatching { Thread.sleep(8) }        // jitter buffer empty → brief wait
            }
            runCatching { track.stop() }; runCatching { track.release() }
            playback = null
        }
    }

    fun stop(ctx: Context? = null) {
        if (!running) return
        running = false
        micOn = false
        runCatching { HeyApi.voiceStop() }
        runCatching {
            ctx?.applicationContext?.let {
                (it.getSystemService(Context.AUDIO_SERVICE) as AudioManager).mode = AudioManager.MODE_NORMAL
            }
        }
        capture = null
        playback = null
    }
}
