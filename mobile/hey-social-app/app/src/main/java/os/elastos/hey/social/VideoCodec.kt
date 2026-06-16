package os.elastos.hey.social

import android.media.Image
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.os.Bundle
import android.view.Surface
import java.nio.ByteBuffer

/**
 * Real-time H.264 ("video/avc") video for a 1:1 Hey call.
 *
 * Two thin wrappers over [MediaCodec]:
 *  - [VideoEncoder]: camera [Image] (YUV_420_888), rotated UPRIGHT -> encoded
 *    H.264 frames. Lazy-configured on the first frame from the source size +
 *    rotation, so a portrait phone never produces a sideways stream.
 *  - [VideoDecoder]: encoded H.264 frames -> rendered to a [Surface].
 *
 * Config (SPS/PPS) is carried IN-BAND: the encoder prepends the codec-specific
 * data to every keyframe, so a receiver can join or recover mid-stream without
 * an out-of-band csd-0/csd-1 exchange.
 *
 * Thread model: the encoder is fed from one camera thread and drained on its own
 * dedicated worker thread; the decoder is fed (and drained) from one thread.
 */

/**
 * Encodes [Image] frames to H.264, rotating each frame UPRIGHT first.
 *
 * The encoder is ROTATION-AWARE and LAZY-CONFIGURED: it does not know the output
 * dimensions until the first [encode] call, because the upright size depends on
 * both the source frame size and CameraX's [rotationDegrees]. On the first frame
 * it derives the output (upright) size, configures + starts the [MediaCodec], and
 * spawns the drain worker.
 *
 * @param bitrateBps   target bitrate (bits/sec); VBR.
 * @param fps          nominal frame rate.
 * @param onEncoded    called from the output worker thread for every encoded
 *                     frame. [isKeyframe] frames already have SPS/PPS prepended.
 */
class VideoEncoder(
    private val bitrateBps: Int,
    private val fps: Int,
    private val onEncoded: (frame: ByteArray, isKeyframe: Boolean) -> Unit,
) {
    private var codec: MediaCodec? = null

    /** Configured output (upright) dimensions; set on the first [encode]. */
    private var outWidth = 0
    private var outHeight = 0

    /** Color format the encoder was actually configured with (device-dependent); drives
     *  the raw-buffer fallback fill when getInputImage is unavailable. */
    private var colorFormat = MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible

    /** Captured SPS/PPS from the CODEC_CONFIG output; prepended to keyframes. */
    @Volatile
    private var csd: ByteArray? = null

    @Volatile
    private var running = false

    /** Set once if the codec could not be configured at any size — stops per-frame
     *  retry spam (the source size is constant for the session). */
    @Volatile
    private var configFailed = false

    private val bufferInfo = MediaCodec.BufferInfo()
    private var drainThread: Thread? = null

    /** Serializes input-buffer fill (CameraX analyzer thread) against codec teardown
     *  (caller thread). MediaCodec is NOT thread-safe: release() frees the native input
     *  buffers, so an in-flight DirectByteBuffer.put would write into unmapped memory
     *  (SIGSEGV SEGV_MAPERR). Mirrors the decoder's decoderLock. */
    private val lock = Any()
    @Volatile private var released = false

    /** Reusable scratch buffers for the YUV rotation; grown on demand. */
    private var yTmp: ByteArray = ByteArray(0)
    private var uTmp: ByteArray = ByteArray(0)
    private var vTmp: ByteArray = ByteArray(0)

    // --- lazy configuration ------------------------------------------------

    /**
     * Pick the upright output size for a [srcW]x[srcH] source frame rotated by
     * [rotationDegrees]. Dimensions are swapped for 90/270, then the long edge is
     * clamped to <=1920 and the short edge to <=1080 (keeping aspect and the
     * portrait/landscape orientation of the rotated frame). Even-aligned for H.264.
     */
    private fun deriveOutputSize(srcW: Int, srcH: Int, rotationDegrees: Int): Pair<Int, Int> {
        val swap = (rotationDegrees == 90 || rotationDegrees == 270)
        var w = if (swap) srcH else srcW
        var h = if (swap) srcW else srcH

        // Clamp long edge <=1920 and short edge <=1080, preserving orientation.
        val longCap = 1920
        val shortCap = 1080
        val isPortrait = h >= w
        val longEdge = if (isPortrait) h else w
        val shortEdge = if (isPortrait) w else h
        var scale = 1.0
        if (longEdge > longCap) scale = minOf(scale, longCap.toDouble() / longEdge)
        if (shortEdge > shortCap) scale = minOf(scale, shortCap.toDouble() / shortEdge)
        if (scale < 1.0) {
            w = (w * scale).toInt()
            h = (h * scale).toInt()
        }

        // H.264 wants even dimensions.
        w = (w / 2) * 2
        h = (h / 2) * 2
        if (w < 2) w = 2
        if (h < 2) h = 2
        return Pair(w, h)
    }

    /**
     * Snap a requested [w]x[h] to the encoder's ACTUAL capabilities: clamp into the
     * supported width/height ranges and round DOWN to the required alignment. Many
     * devices reject an arbitrary derived portrait size (e.g. 1080x1920 or an oddly
     * aligned value) — configuring with a snapped size is what stops the silent
     * config failure that left a phone producing zero frames.
     */
    private fun snapToCaps(
        vc: MediaCodecInfo.VideoCapabilities?,
        w: Int,
        h: Int,
    ): Pair<Int, Int> {
        if (vc == null) return Pair((w / 2) * 2, (h / 2) * 2)
        val wAlign = vc.widthAlignment.coerceAtLeast(2)
        val hAlign = vc.heightAlignment.coerceAtLeast(2)
        var ww = w.coerceIn(vc.supportedWidths.lower, vc.supportedWidths.upper)
        ww = (ww / wAlign) * wAlign
        var hh = h
        runCatching {
            val hr = vc.getSupportedHeightsFor(ww)
            hh = hh.coerceIn(hr.lower, hr.upper)
        }
        hh = (hh / hAlign) * hAlign
        return Pair(ww.coerceAtLeast(wAlign), hh.coerceAtLeast(hAlign))
    }

    /**
     * Configure + start the codec for an upright [reqW]x[reqH] frame, and spawn the
     * output-drain worker. Called once, lazily, from the first [encode]. Tries the
     * requested size (snapped to device caps) then conservative fallbacks; a failure
     * at every size is LOGGED loudly (not swallowed) because it means this side would
     * send no video at all. A fresh MediaCodec is used per attempt (a codec that
     * threw on configure cannot be safely reconfigured).
     */
    /** Pick a color format the encoder ACTUALLY supports. Flexible (device-agnostic via
     *  getInputImage) is preferred; SemiPlanar/Planar are accepted on encoders that don't
     *  advertise Flexible (some Samsung/MediaTek parts) so configure doesn't just fail. */
    private fun pickColorFormat(caps: MediaCodecInfo.CodecCapabilities?): Int {
        val flex = MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible
        val semi = MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420SemiPlanar
        val planar = MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Planar
        val formats = caps?.colorFormats?.toList() ?: return flex
        return when {
            formats.contains(flex) -> flex
            formats.contains(semi) -> semi
            formats.contains(planar) -> planar
            else -> flex
        }
    }

    /** Clamp the requested bitrate into the encoder's supported range (low-end parts cap
     *  well under 4 Mbps and will reject an out-of-range value). */
    private fun clampBitrate(vc: MediaCodecInfo.VideoCapabilities?, bps: Int): Int {
        val r = vc?.bitrateRange ?: return bps
        return bps.coerceIn(r.lower, r.upper)
    }

    /** Clamp the frame rate into what the encoder supports at this size. */
    private fun clampFrameRate(vc: MediaCodecInfo.VideoCapabilities?, w: Int, h: Int, fps: Int): Int =
        runCatching {
            val r = vc?.getSupportedFrameRatesFor(w, h) ?: return fps
            fps.toDouble().coerceIn(r.lower, r.upper).toInt().coerceAtLeast(1)
        }.getOrDefault(fps)

    private fun configureFor(reqW: Int, reqH: Int) {
        val portrait = reqH >= reqW
        val candidates = buildList {
            add(reqW to reqH)
            if (portrait) { add(720 to 1280); add(540 to 960); add(480 to 640) }
            else { add(1280 to 720); add(960 to 540); add(640 to 480) }
        }
        for ((cw, ch) in candidates) {
            val c = runCatching { MediaCodec.createEncoderByType("video/avc") }.getOrNull() ?: continue
            val caps = runCatching { c.codecInfo.getCapabilitiesForType("video/avc") }.getOrNull()
            val vc = caps?.videoCapabilities
            val (w, h) = snapToCaps(vc, cw, ch)
            val cf = pickColorFormat(caps)
            val br = clampBitrate(vc, bitrateBps)
            val fr = clampFrameRate(vc, w, h, fps)
            // Some encoders don't support VBR — fall back to CBR rather than fail configure.
            val vbrOk = caps?.encoderCapabilities?.isBitrateModeSupported(
                MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_VBR,
            ) ?: true
            val ok = runCatching {
                val fmt = MediaFormat.createVideoFormat("video/avc", w, h).apply {
                    setInteger(MediaFormat.KEY_BIT_RATE, br)
                    setInteger(MediaFormat.KEY_FRAME_RATE, fr)
                    setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2)
                    setInteger(
                        MediaFormat.KEY_BITRATE_MODE,
                        if (vbrOk) MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_VBR
                        else MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_CBR,
                    )
                    setInteger(MediaFormat.KEY_COLOR_FORMAT, cf)
                }
                c.configure(fmt, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                c.start()
            }.isSuccess
            if (ok) {
                outWidth = w
                outHeight = h
                colorFormat = cf
                codec = c
                running = true
                drainThread = Thread({ drainLoop() }, "hey-venc-drain").apply {
                    isDaemon = true
                    start()
                }
                android.util.Log.i("video", "encoder configured ${w}x${h} color=$cf br=$br fps=$fr (req ${cw}x${ch})")
                return
            }
            android.util.Log.w("video", "encoder config ${w}x${h} (req ${cw}x${ch}) failed; trying fallback")
            runCatching { c.release() }
        }
        android.util.Log.e("video", "encoder could NOT configure at any size — THIS side will send no video")
    }

    /**
     * Rotate one camera [image] (YUV_420_888) UPRIGHT by [rotationDegrees]
     * (0/90/180/270), copy it into a free input buffer and queue it.
     *
     * On the FIRST call the codec is configured for the resulting upright size.
     * The source is read synchronously here, so the caller may close the [image]
     * immediately on return. If no input buffer is currently free the frame is
     * dropped (this is real-time video; never block the camera).
     */
    fun encode(image: Image, ptsUs: Long, rotationDegrees: Int) {
        val rot = ((rotationDegrees % 360) + 360) % 360
        // Lazy-configure once we know the source size + rotation. Try ONCE: the source
        // size is constant for the session, so a total config failure won't recover by
        // retrying — it'd only spam. configureFor logs its own outcome.
        if (codec == null) {
            if (configFailed) return
            val (w, h) = deriveOutputSize(image.width, image.height, rot)
            runCatching { configureFor(w, h) }
            if (codec == null) {
                configFailed = true
                return
            }
        }
        val c = codec ?: return

        // Rotate the source into densely-packed I420 scratch (yTmp/uTmp/vTmp)
        // sized to the codec's OUTPUT dimensions.
        val rotated = runCatching { rotateToI420(image, rot, outWidth, outHeight) }.getOrNull()
        if (rotated != true) return

        // Hold `lock` across the whole dequeue->fill->queue so release() (which frees the
        // codec's native input buffers) cannot run mid-put. `if (released)` makes a frame
        // delivered during teardown a no-op instead of a write into freed memory.
        synchronized(lock) {
            if (released) return
            val index = try {
                c.dequeueInputBuffer(10_000L) // 10ms
            } catch (t: Throwable) {
                return
            }
            if (index < 0) return // no free input buffer -> drop this frame

            try {
                val dst: Image? = runCatching { c.getInputImage(index) }.getOrNull()
                val sizeFilled: Int = when {
                    // getInputImage adapts to whatever Flexible/SemiPlanar layout the device uses.
                    dst != null -> fillInputImageFromI420(dst, outWidth, outHeight)
                    // Raw-buffer fallback: match the CONFIGURED color format so colors are right.
                    colorFormat == MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Planar ->
                        fillI420(c.getInputBuffer(index), outWidth, outHeight)
                    else -> fillNv12FromI420(c.getInputBuffer(index), outWidth, outHeight)
                }
                c.queueInputBuffer(index, 0, sizeFilled, ptsUs, 0)
            } catch (t: Throwable) {
                // Return the buffer empty so the codec isn't starved by a leaked index.
                runCatching { c.queueInputBuffer(index, 0, 0, ptsUs, 0) }
            }
        }
    }

    /** Ask the encoder to emit a keyframe as soon as possible. */
    fun requestKeyframe() {
        runCatching {
            codec?.setParameters(
                Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) },
            )
        }
    }

    /** Re-target the encoder's bitrate (bits/sec) on the fly. */
    fun setBitrate(bps: Int) {
        runCatching {
            codec?.setParameters(
                Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_VIDEO_BITRATE, bps) },
            )
        }
    }

    /** Stop the drain loop and release the codec. Safe to call once. */
    fun release() {
        running = false
        runCatching { drainThread?.join(500L) }
        // Block any in-flight encode() fill before freeing the codec, and make any later
        // encode() a no-op (the analyzer thread may still deliver one more frame).
        synchronized(lock) {
            released = true
            runCatching { codec?.stop() }
            runCatching { codec?.release() }
            codec = null
        }
    }

    // --- output draining ---------------------------------------------------

    private fun drainLoop() {
        val codec = this.codec ?: return
        while (running) {
            val index = try {
                codec.dequeueOutputBuffer(bufferInfo, 10_000L) // 10ms
            } catch (t: Throwable) {
                // Codec may be tearing down; bail out of the loop.
                break
            }
            if (index < 0) {
                // INFO_TRY_AGAIN_LATER / INFO_OUTPUT_FORMAT_CHANGED / etc.
                continue
            }

            try {
                val out: ByteBuffer? = codec.getOutputBuffer(index)
                val flags = bufferInfo.flags
                val isConfig = (flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0

                if (isConfig) {
                    // Capture SPS/PPS; don't emit it as a media frame.
                    if (out != null && bufferInfo.size > 0) {
                        out.position(bufferInfo.offset)
                        out.limit(bufferInfo.offset + bufferInfo.size)
                        csd = ByteArray(bufferInfo.size).also { out.get(it) }
                    }
                } else if (out != null && bufferInfo.size > 0) {
                    out.position(bufferInfo.offset)
                    out.limit(bufferInfo.offset + bufferInfo.size)
                    val payload = ByteArray(bufferInfo.size).also { out.get(it) }

                    val isKeyframe = (flags and MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0
                    val header = csd
                    val frame = if (isKeyframe && header != null) {
                        ByteArray(header.size + payload.size).also { merged ->
                            System.arraycopy(header, 0, merged, 0, header.size)
                            System.arraycopy(payload, 0, merged, header.size, payload.size)
                        }
                    } else {
                        payload
                    }
                    runCatching { onEncoded(frame, isKeyframe) }
                }
            } finally {
                runCatching { codec.releaseOutputBuffer(index, false) }
            }
        }
    }

    // --- YUV rotation ------------------------------------------------------

    /**
     * Rotate [src] (YUV_420_888) by [rot] degrees (0/90/180/270) into the
     * densely-packed I420 scratch planes [yTmp]/[uTmp]/[vTmp], sized to the
     * codec OUTPUT dimensions [dstW]x[dstH].
     *
     * Both the source rowStride/pixelStride and the half-resolution chroma layout
     * are honored. For 90/270 the planes are transposed (with the appropriate flip
     * so the picture comes out upright, not mirrored). [dstW]x[dstH] equals the
     * rotated source size (possibly downscaled+even-clamped); when the rotated
     * source is larger than the configured output we sample the centred top-left
     * region (nearest), so a clamped 1080p output stays correct.
     *
     * @return true on success.
     */
    private fun rotateToI420(src: Image, rot: Int, dstW: Int, dstH: Int): Boolean {
        val srcW = src.width
        val srcH = src.height
        if (srcW <= 0 || srcH <= 0 || dstW <= 0 || dstH <= 0) return false

        // The rotated (un-clamped) source size, in upright orientation.
        val swap = (rot == 90 || rot == 270)
        val rotW = if (swap) srcH else srcW
        val rotH = if (swap) srcW else srcH

        // Map output (dst) pixel -> rotated-source pixel via nearest scaling.
        // Then rotated-source pixel -> original-source pixel via the inverse
        // rotation. Chroma is half-res in BOTH dims and uses the SAME mapping
        // at half scale.
        val cDstW = (dstW + 1) / 2
        val cDstH = (dstH + 1) / 2

        if (yTmp.size < dstW * dstH) yTmp = ByteArray(dstW * dstH)
        if (uTmp.size < cDstW * cDstH) uTmp = ByteArray(cDstW * cDstH)
        if (vTmp.size < cDstW * cDstH) vTmp = ByteArray(cDstW * cDstH)

        val yP = src.planes[0]
        val uP = src.planes[1]
        val vP = src.planes[2]
        val yBuf = yP.buffer
        val uBuf = uP.buffer
        val vBuf = vP.buffer
        val yRow = yP.rowStride
        val yPix = yP.pixelStride
        val uRow = uP.rowStride
        val uPix = uP.pixelStride
        val vRow = vP.rowStride
        val vPix = vP.pixelStride
        val cSrcW = (srcW + 1) / 2
        val cSrcH = (srcH + 1) / 2

        // --- Y plane (full res) ---
        for (dy in 0 until dstH) {
            // Nearest map dst row -> rotated-source row.
            val ry = if (dstH == rotH) dy else (dy.toLong() * rotH / dstH).toInt()
            val dstRowBase = dy * dstW
            for (dx in 0 until dstW) {
                val rx = if (dstW == rotW) dx else (dx.toLong() * rotW / dstW).toInt()
                // Inverse-rotate (rx,ry) in rotated space back to source (sx,sy).
                val sx: Int
                val sy: Int
                when (rot) {
                    90 -> { sx = ry; sy = rotW - 1 - rx }
                    180 -> { sx = srcW - 1 - rx; sy = srcH - 1 - ry }
                    270 -> { sx = rotH - 1 - ry; sy = rx }
                    else -> { sx = rx; sy = ry }
                }
                val cx = if (sx < 0) 0 else if (sx >= srcW) srcW - 1 else sx
                val cy = if (sy < 0) 0 else if (sy >= srcH) srcH - 1 else sy
                val idx = cy * yRow + cx * yPix
                yTmp[dstRowBase + dx] = if (idx < yBuf.limit()) yBuf.get(idx) else 0
            }
        }

        // --- Chroma planes (half res in both dims) ---
        val cRotW = (rotW + 1) / 2
        val cRotH = (rotH + 1) / 2
        for (dy in 0 until cDstH) {
            val ry = if (cDstH == cRotH) dy else (dy.toLong() * cRotH / cDstH).toInt()
            val dstRowBase = dy * cDstW
            for (dx in 0 until cDstW) {
                val rx = if (cDstW == cRotW) dx else (dx.toLong() * cRotW / cDstW).toInt()
                val sx: Int
                val sy: Int
                when (rot) {
                    90 -> { sx = ry; sy = cRotW - 1 - rx }
                    180 -> { sx = cSrcW - 1 - rx; sy = cSrcH - 1 - ry }
                    270 -> { sx = cRotH - 1 - ry; sy = rx }
                    else -> { sx = rx; sy = ry }
                }
                val cx = if (sx < 0) 0 else if (sx >= cSrcW) cSrcW - 1 else sx
                val cy = if (sy < 0) 0 else if (sy >= cSrcH) cSrcH - 1 else sy
                val uIdx = cy * uRow + cx * uPix
                val vIdx = cy * vRow + cx * vPix
                uTmp[dstRowBase + dx] = if (uIdx < uBuf.limit()) uBuf.get(uIdx) else 0
                vTmp[dstRowBase + dx] = if (vIdx < vBuf.limit()) vBuf.get(vIdx) else 0
            }
        }
        return true
    }

    /**
     * Write the densely-packed I420 scratch ([yTmp]/[uTmp]/[vTmp], sized
     * [w]x[h]) into the codec input [dst] [Image], honoring the destination's
     * own rowStride/pixelStride (interleaved or planar U/V).
     *
     * @return total number of bytes written.
     */
    private fun fillInputImageFromI420(dst: Image, w: Int, h: Int): Int {
        var total = 0
        val cW = (w + 1) / 2
        val cH = (h + 1) / 2
        for (p in 0..2) {
            val plane = dst.planes[p]
            val buf = plane.buffer
            val rowStride = plane.rowStride
            val pixStride = plane.pixelStride
            val srcArr = when (p) {
                0 -> yTmp
                1 -> uTmp
                else -> vTmp
            }
            val planeW = if (p == 0) w else cW
            val planeH = if (p == 0) h else cH
            for (row in 0 until planeH) {
                val srcRowBase = row * planeW
                val dstRowBase = row * rowStride
                if (pixStride == 1) {
                    if (dstRowBase < buf.limit()) {
                        buf.position(dstRowBase)
                        val n = minOf(planeW, buf.remaining())
                        buf.put(srcArr, srcRowBase, n)
                        total += n
                    }
                } else {
                    for (col in 0 until planeW) {
                        val idx = dstRowBase + col * pixStride
                        if (idx < buf.limit()) {
                            buf.put(idx, srcArr[srcRowBase + col])
                            total++
                        }
                    }
                }
            }
        }
        return total
    }

    /**
     * Fallback path: pack the I420 scratch ([yTmp]/[uTmp]/[vTmp], sized [w]x[h])
     * into [dst] as NV12 (full Y plane followed by interleaved U,V,U,V...). Used
     * only when [MediaCodec.getInputImage] is unavailable for the dequeued index.
     *
     * @return number of bytes written.
     */
    private fun fillNv12FromI420(dst: ByteBuffer?, w: Int, h: Int): Int {
        if (dst == null) return 0
        dst.clear()
        var written = 0
        // Y plane (densely packed already).
        val ySize = w * h
        val yN = minOf(ySize, dst.remaining())
        dst.put(yTmp, 0, yN)
        written += yN

        // Interleaved UV plane.
        val cW = (w + 1) / 2
        val cH = (h + 1) / 2
        for (row in 0 until cH) {
            val base = row * cW
            for (col in 0 until cW) {
                if (dst.remaining() < 2) return written
                dst.put(uTmp[base + col])
                dst.put(vTmp[base + col])
                written += 2
            }
        }
        return written
    }

    /**
     * Fallback for a codec configured as planar I420 (separate Y, U, V): the scratch is
     * already I420, so write Y then U then V contiguously. Only used when getInputImage
     * is null AND the chosen color format is Planar (rare).
     */
    private fun fillI420(dst: ByteBuffer?, w: Int, h: Int): Int {
        if (dst == null) return 0
        dst.clear()
        var written = 0
        val ySize = w * h
        val yN = minOf(ySize, dst.remaining()); dst.put(yTmp, 0, yN); written += yN
        val cSize = ((w + 1) / 2) * ((h + 1) / 2)
        val uN = minOf(cSize, dst.remaining()); dst.put(uTmp, 0, uN); written += uN
        val vN = minOf(cSize, dst.remaining()); dst.put(vTmp, 0, vN); written += vN
        return written
    }
}

/**
 * Decodes H.264 frames and renders them to [surface].
 *
 * Created with a nominal size; the decoder adapts to the real resolution from
 * the in-band SPS/PPS that the [VideoEncoder] prepends to keyframes, so no
 * up-front csd-0/csd-1 is required.
 */
class VideoDecoder(
    private val surface: Surface,
    private val onSize: ((width: Int, height: Int) -> Unit)? = null,
    /** Invoked after the decoder is reset following an error, so the caller re-arms its
     *  keyframe gate (a reset decoder must restart on a keyframe). */
    private val onReset: (() -> Unit)? = null,
) {
    private val codec: MediaCodec = MediaCodec.createDecoderByType("video/avc")
    private val bufferInfo = MediaCodec.BufferInfo()

    init {
        // Nominal size; real dimensions come from in-band SPS/PPS.
        val fmt = MediaFormat.createVideoFormat("video/avc", 1280, 720)
        codec.configure(fmt, surface, null, 0)
        codec.start()
    }

    /**
     * Queue one encoded H.264 [frame] and render any decoded output to the
     * surface. A keyframe carries its own SPS/PPS so the decoder can start or
     * recover from this call alone.
     */
    fun feed(frame: ByteArray, ptsUs: Long) {
        try {
            val index = codec.dequeueInputBuffer(10_000L) // 10ms
            if (index >= 0) {
                val input: ByteBuffer? = codec.getInputBuffer(index)
                if (input != null) {
                    input.clear()
                    input.put(frame)
                }
                codec.queueInputBuffer(index, 0, frame.size, ptsUs, 0)
            }

            // Drain whatever decoded output is ready and render it.
            while (true) {
                val outIndex = codec.dequeueOutputBuffer(bufferInfo, 0L)
                when {
                    outIndex >= 0 -> {
                        runCatching { codec.releaseOutputBuffer(outIndex, true) } // render = true
                    }
                    outIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        // Decoder picked up real dimensions from in-band SPS/PPS. Report the
                        // VISIBLE size (crop rect when present, else coded W×H) so the UI can
                        // render aspect-correct (no stretch) instead of guessing.
                        runCatching {
                            val f = codec.outputFormat
                            val cl = if (f.containsKey("crop-left")) f.getInteger("crop-left") else 0
                            val cr = if (f.containsKey("crop-right")) f.getInteger("crop-right") else -1
                            val ct = if (f.containsKey("crop-top")) f.getInteger("crop-top") else 0
                            val cb = if (f.containsKey("crop-bottom")) f.getInteger("crop-bottom") else -1
                            val w = if (cr >= cl) (cr - cl + 1) else f.getInteger(MediaFormat.KEY_WIDTH)
                            val h = if (cb >= ct) (cb - ct + 1) else f.getInteger(MediaFormat.KEY_HEIGHT)
                            if (w > 0 && h > 0) {
                                android.util.Log.i("video", "decoder output size ${w}x${h}")
                                onSize?.invoke(w, h)
                            }
                        }
                    }
                    else -> {
                        // INFO_TRY_AGAIN_LATER (or INFO_OUTPUT_BUFFERS_CHANGED): nothing now.
                        break
                    }
                }
            }
        } catch (e: MediaCodec.CodecException) {
            // A chipset decode error (often transient). Reset + re-arm so the call
            // self-heals on the next keyframe instead of going permanently black.
            android.util.Log.w("video", "decoder CodecException (recoverable=${e.isRecoverable}); resetting")
            recover()
        } catch (t: Throwable) {
            // Malformed frame / buffer overflow — skip it; the next keyframe recovers.
        }
    }

    /** Reset + reconfigure the decoder in place after an error, then ask the caller to
     *  re-arm its keyframe gate (a reset decoder must restart from a keyframe). */
    private fun recover() {
        runCatching {
            codec.reset()
            val fmt = MediaFormat.createVideoFormat("video/avc", 1280, 720)
            codec.configure(fmt, surface, null, 0)
            codec.start()
            onReset?.invoke()
        }.onFailure { android.util.Log.e("video", "decoder recover failed", it) }
    }

    /** Stop and release the codec. Safe to call once. */
    fun release() {
        runCatching { codec.stop() }
        runCatching { codec.release() }
    }
}
