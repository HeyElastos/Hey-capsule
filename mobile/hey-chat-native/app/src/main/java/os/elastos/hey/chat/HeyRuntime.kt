package os.elastos.hey.chat

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Bridge to the embedded Rust mini-runtime (libhey_mobile_runtime.so).
 *
 * The whole carrier + identity + storage + content + loopback HTTP server runs
 * IN THIS PROCESS on a background thread the native side spawns. There is no
 * external runtime, no wallet, no kubo, no child processes. Kotlin's only jobs:
 * stage the WASM `dist/` onto the filesystem (ServeDir needs a real path) and
 * call nativeStart once.
 */
object HeyRuntime {
    // Distinct port from Hey Social (8787): on Android 127.0.0.1 is shared
    // device-wide, so the two apps must not both bind the same loopback port.
    const val PORT = 8788
    const val URL = "http://127.0.0.1:$PORT/apps/hey-chat/"
    private const val TAG = "HeyRuntime"

    @Volatile private var started = false

    init {
        System.loadLibrary("hey_mobile_runtime")
    }

    /**
     * @param dataDir       app-private writable dir (carrier keys, identity, storage)
     * @param distDir       directory holding the capsule's built dist/
     * @param port          loopback port to bind
     * @param identityBlob  Keystore-unlocked identity JSON, or null to load/create locally
     * @return the bound port
     */
    external fun nativeStart(dataDir: String, distDir: String, port: Int, identityBlob: String?): Int

    /** Idempotent: stage assets + start the runtime exactly once per process. */
    @Synchronized
    fun ensureStarted(ctx: Context) {
        if (started) return
        val dataDir = File(ctx.filesDir, "hey").apply { mkdirs() }
        val distDir = stageDist(ctx)
        // identityBlob = null for v1 (local identity.json in dataDir). The
        // StrongBox/biometric path will decrypt the vault and pass it here.
        val bound = nativeStart(dataDir.absolutePath, distDir.absolutePath, PORT, null)
        Log.i(TAG, "runtime started on port $bound, data=$dataDir")
        started = true
    }

    /**
     * Copy the bundled `assets/dist` into filesDir/dist so the native ServeDir
     * can serve it. Re-copies whenever the APK's dist changes (tracked by a
     * marker file holding the app versionCode).
     */
    private fun stageDist(ctx: Context): File {
        val out = File(ctx.filesDir, "dist")
        val marker = File(out, ".version")
        val version = ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionCode.toString()
        if (out.isDirectory && marker.takeIf { it.exists() }?.readText() == version) {
            return out
        }
        out.deleteRecursively()
        out.mkdirs()
        copyAsset(ctx, "dist", out)
        marker.writeText(version)
        Log.i(TAG, "staged dist -> $out")
        return out
    }

    private fun copyAsset(ctx: Context, assetPath: String, destDir: File) {
        val am = ctx.assets
        val children = am.list(assetPath) ?: emptyArray()
        if (children.isEmpty()) {
            // It's a file: copy it.
            destDir.parentFile?.mkdirs()
            am.open(assetPath).use { input ->
                destDir.outputStream().use { output -> input.copyTo(output) }
            }
            return
        }
        destDir.mkdirs()
        for (child in children) {
            copyAsset(ctx, "$assetPath/$child", File(destDir, child))
        }
    }
}
