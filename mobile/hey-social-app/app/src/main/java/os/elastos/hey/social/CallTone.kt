package os.elastos.hey.social

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import kotlin.concurrent.thread
import kotlin.math.exp
import kotlin.math.sin

/**
 * Incoming-call ringtone — synthesized, no audio asset. A calm, modern bell-like two-note chime
 * (D5 → G5, a soft perfect fourth) with an exponential decay envelope + a gentle 2nd harmonic for
 * warmth, followed by a pause, looped. Routed to the ring stream. Start on an incoming call, stop on
 * accept/decline/timeout.
 */
object CallTone {
    private const val RATE = 44100

    @Volatile private var playing = false
    @Volatile private var track: AudioTrack? = null

    fun startIncoming(ctx: Context) {
        if (playing) return
        playing = true
        thread(name = "call-tone") {
            val pcm = buildChime()
            val frames = pcm.size / 2
            val t = runCatching {
                AudioTrack.Builder()
                    .setAudioAttributes(
                        AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                            .build()
                    )
                    .setAudioFormat(
                        AudioFormat.Builder()
                            .setSampleRate(RATE)
                            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                            .build()
                    )
                    .setBufferSizeInBytes(pcm.size)
                    .setTransferMode(AudioTrack.MODE_STATIC)
                    .build()
            }.getOrNull() ?: run { playing = false; return@thread }
            runCatching {
                t.write(pcm, 0, pcm.size)
                t.setLoopPoints(0, frames, -1) // loop forever (the buffer ends in silence = the gap)
                track = t
                t.play()
            }
            while (playing) runCatching { Thread.sleep(120) }
            runCatching { t.stop() }; runCatching { t.release() }
            track = null
        }
    }

    fun stop() {
        playing = false
        runCatching { track?.stop() }
    }

    /** One ring cycle: two soft decaying notes + a trailing pause (≈2.6 s total). */
    private fun buildChime(): ByteArray {
        val total = (RATE * 2.6).toInt()
        val buf = DoubleArray(total)
        fun note(startSec: Double, durSec: Double, freq: Double, vol: Double) {
            val start = (startSec * RATE).toInt()
            val dur = (durSec * RATE).toInt()
            for (i in 0 until dur) {
                val idx = start + i
                if (idx >= total) break
                val tt = i.toDouble() / RATE
                // soft attack (~12 ms) + bell-like exponential decay
                val env = (1.0 - exp(-80.0 * tt)) * exp(-2.6 * tt)
                val s = sin(2 * Math.PI * freq * tt) + 0.28 * sin(2 * Math.PI * freq * 2 * tt)
                buf[idx] += s * env * vol
            }
        }
        note(0.0, 1.3, 587.33, 0.75)  // D5
        note(0.30, 1.3, 783.99, 0.62) // G5 (perfect fourth above), slightly later + softer
        val out = ByteArray(total * 2)
        for (i in 0 until total) {
            val v = (buf[i].coerceIn(-1.0, 1.0) * 10000).toInt() // headroom — calm, not loud
            out[i * 2] = (v and 0xFF).toByte()
            out[i * 2 + 1] = ((v shr 8) and 0xFF).toByte()
        }
        return out
    }
}
