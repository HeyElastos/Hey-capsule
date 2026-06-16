package os.elastos.hey.social

import android.content.Context
import android.media.Image
import android.util.Size
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * Camera capture for a real-time 1:1 video call.
 *
 * Binds two CameraX use cases to a single camera:
 *  1. A [Preview] driving [previewView] for the on-screen local self-view (optional).
 *  2. An [ImageAnalysis] (YUV_420_888, ~1080p, keep-only-latest) that hands each
 *     frame to [onImage] for H.264 encoding.
 *
 * Threading: provider resolution + binding happen on the main executor; the analyzer
 * runs on a dedicated single-thread background executor.
 *
 * IMPORTANT: [onImage] is invoked **synchronously** while the underlying
 * [androidx.camera.core.ImageProxy] is still open. The consumer MUST copy whatever it
 * needs before returning — the [Image] is invalid once the proxy is closed (which this
 * class always does in a `finally`).
 *
 * @param ctx application/activity context.
 * @param lifecycleOwner lifecycle the camera use cases are bound to.
 * @param previewView optional view for the local self-preview; pass `null` to skip it.
 * @param onImage per-frame callback `(image, ptsUs)` where `ptsUs` is the frame
 *   timestamp in microseconds. Consume synchronously.
 */
class VideoCapture(
    private val ctx: Context,
    private val previewView: PreviewView?,
    private val onImage: (image: Image, ptsUs: Long, rotationDegrees: Int) -> Unit,
) : LifecycleOwner {
    // Own the lifecycle the camera binds to. PiP (and the Activity going to onStop while
    // the floating window is up) would otherwise drop the host lifecycle below STARTED and
    // make CameraX auto-unbind the ImageAnalysis use case — silently freezing the OUTGOING
    // video to the peer. We force RESUMED for the whole call so capture+encode keep running
    // across PiP, and DESTROYED only on release() (real call end).
    private val registry = LifecycleRegistry(this)
    override val lifecycle: Lifecycle get() = registry

    /** Target capture resolution; CameraX falls back to the nearest supported size. */
    private val targetResolution = Size(1920, 1080)

    /** Background executor for the [ImageAnalysis] analyzer. */
    private val analysisExecutor: ExecutorService = Executors.newSingleThreadExecutor()

    /** Resolved once the [ProcessCameraProvider] future completes. */
    @Volatile
    private var cameraProvider: ProcessCameraProvider? = null

    /** `true` = front (selfie) camera, `false` = back. Defaults to front. */
    @Volatile
    private var front: Boolean = true

    /** Guards against shutting down twice. */
    @Volatile
    private var released: Boolean = false

    /**
     * Acquire the camera provider and bind the use cases (front camera by default).
     * Safe to call before the camera is ready — binding is deferred to the future.
     */
    fun start() {
        if (released) return
        // Drive our own lifecycle to RESUMED so CameraX keeps the camera bound across PiP /
        // Activity pause. Set before bind() runs on the main executor. (start() is called on
        // the main thread from the call composable.)
        registry.currentState = Lifecycle.State.RESUMED
        val future = ProcessCameraProvider.getInstance(ctx)
        future.addListener({
            runCatching {
                if (released) return@runCatching
                cameraProvider = future.get()
                bind()
            }
        }, ContextCompat.getMainExecutor(ctx))
    }

    /**
     * Toggle between front and back cameras and rebind. No-op if the provider is not
     * yet available or this instance has been released.
     */
    fun flip() {
        if (released) return
        front = !front
        runCatching { bind() }
    }

    /**
     * Unbind all use cases and shut down the analyzer executor. Idempotent.
     */
    fun release() {
        if (released) return
        released = true
        // DESTROYED unbinds CameraX; this is the ONLY thing that stops capture (call end).
        runCatching { registry.currentState = Lifecycle.State.DESTROYED }
        runCatching { cameraProvider?.unbindAll() }
        // Wait for the analyzer thread to finish any in-flight frame BEFORE the encoder's
        // codec is released, so its DirectByteBuffer.put can't write into a freed buffer.
        runCatching {
            analysisExecutor.shutdown()
            analysisExecutor.awaitTermination(500, java.util.concurrent.TimeUnit.MILLISECONDS)
        }
    }

    /**
     * Build and bind the Preview + ImageAnalysis use cases to [lifecycleOwner] using
     * the currently selected camera. Must run on the main thread (CameraX requirement).
     */
    private fun bind() {
        val provider = cameraProvider ?: return
        if (released) return

        val selector =
            if (front) CameraSelector.DEFAULT_FRONT_CAMERA else CameraSelector.DEFAULT_BACK_CAMERA

        // ResolutionSelector is the non-deprecated CameraX 1.3.4 resolution API
        // (setTargetResolution is deprecated). Target 1080p, fall back to nearest.
        val resolutionSelector = ResolutionSelector.Builder()
            .setResolutionStrategy(
                ResolutionStrategy(
                    targetResolution,
                    ResolutionStrategy.FALLBACK_RULE_CLOSEST_HIGHER_THEN_LOWER,
                )
            )
            .build()

        val analysis = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_YUV_420_888)
            .setResolutionSelector(resolutionSelector)
            .build()
            .also { ia ->
                ia.setAnalyzer(analysisExecutor) { proxy ->
                    try {
                        if (released) return@setAnalyzer // dropped during teardown; finally still closes
                        val img = proxy.image
                        if (img != null) {
                            // imageInfo.timestamp is in nanoseconds; convert ns -> us.
                            val ptsUs = proxy.imageInfo.timestamp / 1000
                            val rot = proxy.imageInfo.rotationDegrees // rotate to upright in the encoder
                            runCatching { onImage(img, ptsUs, rot) }
                        }
                    } finally {
                        // The Image is invalid after this; consumer must have copied
                        // synchronously. Always close so the pipeline keeps flowing.
                        proxy.close()
                    }
                }
            }

        runCatching {
            provider.unbindAll()

            if (previewView != null) {
                val preview = Preview.Builder()
                    .setResolutionSelector(resolutionSelector)
                    .build()
                    .also { it.setSurfaceProvider(previewView.surfaceProvider) }
                provider.bindToLifecycle(this, selector, preview, analysis)
            } else {
                provider.bindToLifecycle(this, selector, analysis)
            }
        }
    }
}
