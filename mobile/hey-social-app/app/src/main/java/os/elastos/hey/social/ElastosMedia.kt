package os.elastos.hey.social

import android.app.Application
import android.net.Uri
import android.os.StrictMode
import coil.ImageLoader
import coil.ImageLoaderFactory
import coil.decode.DataSource
import coil.decode.ImageSource
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.fetch.SourceResult
import coil.request.Options
import okio.Buffer
import java.io.IOException

/**
 * Coil fetcher for media addressed as `localhost://WebSpaces/hey/<cid>` — a
 * user's personal WebSpace drive (the PC2 data plane). Media in the UI is
 * addressed by namespace, never by network (no 127.0.0.1, no /ipfs gateway, no
 * IP/port). This resolves the handle's cid to bytes through the in-process
 * content provider (elastos content API), so the runtime + carrier keep the
 * network hidden exactly as the Internet OS model intends.
 */
class WebSpaceFetcher(private val data: Uri, private val options: Options) : Fetcher {
    override suspend fun fetch(): FetchResult {
        val cid = data.toString().substringAfterLast('/').trim()
        val bytes = HeyApi.contentBytes(cid)
        if (bytes.isEmpty()) throw IOException("no content for $data")
        return SourceResult(
            source = ImageSource(Buffer().apply { write(bytes) }, options.context),
            mimeType = null,
            dataSource = DataSource.DISK, // immutable, content-addressed → cacheable
        )
    }

    class Factory : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? =
            if (data.scheme == "localhost" && data.host == "WebSpaces") WebSpaceFetcher(data, options) else null
    }
}

/** Registers the WebSpace media resolver as the app-wide Coil loader. */
class HeyApplication : Application(), ImageLoaderFactory {
    override fun onCreate() {
        super.onCreate()
        // Debug-only leak + misuse detection: StrictMode surfaces leaked
        // Closeables/Activities/Fragments + disk/network on the main thread.
        // penaltyLog (not penaltyDeath) -> logged, never crashes a debug run.
        // Absent from release entirely via the BuildConfig.DEBUG guard.
        if (BuildConfig.DEBUG) {
            StrictMode.setThreadPolicy(
                StrictMode.ThreadPolicy.Builder().detectAll().penaltyLog().build()
            )
            StrictMode.setVmPolicy(
                StrictMode.VmPolicy.Builder().detectAll().penaltyLog().build()
            )
        }
    }

    override fun newImageLoader(): ImageLoader =
        ImageLoader.Builder(this)
            .components { add(WebSpaceFetcher.Factory()) }
            .build()
}
