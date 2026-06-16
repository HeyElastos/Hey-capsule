package os.elastos.hey.social

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.Gravity
import android.webkit.ConsoleMessage
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.ScrollView
import android.widget.TextView
import android.app.Activity
import android.app.AlertDialog
import kotlin.concurrent.thread
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * The entire app surface: one full-screen WebView pointed at the in-process
 * runtime. Users never see "a runtime" or "a carrier" — they open Hey Social
 * and the real Leptos/WASM UI loads from http://127.0.0.1:8787/apps/hey-social/.
 */
class MainActivity : Activity() {

    private lateinit var webView: WebView

    // Pending <input type=file> callback — the WebView won't open a picker on
    // its own, so we launch one and feed the chosen URIs back through this.
    private var filePathCallback: ValueCallback<Array<Uri>>? = null
    private val fileChooserCode = 1001
    private var statusView: TextView? = null
    // Captured WebView JS console (shown in the on-device log viewer).
    private val consoleLog = java.util.Collections.synchronizedList(ArrayList<String>())

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // 1. Start the embedded runtime (carrier + identity + server) and keep
        //    the process alive in the background via a foreground service.
        HeyRuntime.ensureStarted(applicationContext)
        startRuntimeService()

        // VERBOSE MODE: surface everything the WASM UI does into logcat, and
        // enable chrome://inspect remote debugging. This is what turns a "blank
        // page" into an actionable error.
        WebView.setWebContentsDebuggingEnabled(true)

        // 2. WebView that runs the unmodified hey-social WASM.
        webView = WebView(this).apply {
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.databaseEnabled = true
            settings.allowFileAccess = true
            settings.allowContentAccess = true
            settings.mediaPlaybackRequiresUserGesture = false

            // JS console (Leptos panic hook + boot_log) -> logcat tag "HeyWeb".
            webChromeClient = object : WebChromeClient() {
                override fun onConsoleMessage(msg: ConsoleMessage): Boolean {
                    val line = "[${msg.messageLevel()}] ${msg.message()} (${msg.sourceId()}:${msg.lineNumber()})"
                    Log.i("HeyWeb", line)
                    consoleLog.add(line)
                    if (consoleLog.size > 400) consoleLog.removeAt(0)
                    return true
                }

                // Photo/video upload: open the Android picker and hand the
                // selected URIs back to the WASM app's <input type=file>.
                override fun onShowFileChooser(
                    view: WebView,
                    callback: ValueCallback<Array<Uri>>,
                    params: FileChooserParams
                ): Boolean {
                    filePathCallback?.onReceiveValue(null) // cancel any stale one
                    filePathCallback = callback
                    return try {
                        val intent = params.createIntent()
                        intent.addCategory(Intent.CATEGORY_OPENABLE)
                        startActivityForResult(intent, fileChooserCode)
                        true
                    } catch (e: Exception) {
                        Log.e("HeyWeb", "file chooser failed: $e")
                        filePathCallback = null
                        false
                    }
                }
            }

            // Network/navigation errors (failed fetches, 4xx/5xx) -> logcat.
            webViewClient = object : WebViewClient() {
                override fun onReceivedError(
                    view: WebView, req: WebResourceRequest, err: WebResourceError
                ) {
                    Log.e("HeyWeb", "load error ${err.errorCode} ${err.description} @ ${req.url}")
                }
                override fun onReceivedHttpError(
                    view: WebView, req: WebResourceRequest, resp: WebResourceResponse
                ) {
                    Log.e("HeyWeb", "http ${resp.statusCode} @ ${req.url}")
                }
                override fun onPageFinished(view: WebView, url: String) {
                    Log.i("HeyWeb", "page finished: $url")
                }
            }
        }
        // Native status pill overlaid on the WebView — shows whether the
        // embedded carrier/iroh is up and how many peers are meshed.
        val status = TextView(this).apply {
            text = "● starting…"
            setTextColor(Color.WHITE)
            setBackgroundColor(0xCC000000.toInt())
            textSize = 11f
            setPadding(20, 10, 20, 10)
        }
        status.setOnClickListener { showDebugLogs() }
        statusView = status
        val root = FrameLayout(this).apply {
            addView(webView)
            addView(
                status,
                FrameLayout.LayoutParams(
                    FrameLayout.LayoutParams.WRAP_CONTENT,
                    FrameLayout.LayoutParams.WRAP_CONTENT
                ).apply {
                    gravity = Gravity.TOP or Gravity.END
                    topMargin = 60
                    rightMargin = 24
                }
            )
        }
        setContentView(root)

        // 3. Wait until the loopback server is accepting connections, then load.
        thread(name = "hey-await-runtime") {
            waitForPort(HeyRuntime.PORT, timeoutMs = 20_000)
            runOnUiThread { webView.loadUrl(HeyRuntime.URL) }
        }

        // 4. Poll carrier status for the pill.
        thread(name = "hey-status-poll") {
            while (true) {
                val s = fetchStatus()
                runOnUiThread { renderStatus(s) }
                Thread.sleep(3000)
            }
        }
    }

    /** Tap the status pill → on-device log viewer (runtime ring + JS console). */
    private fun showDebugLogs() {
        thread(name = "hey-logs") {
            val runtime = try {
                val conn = URL("http://127.0.0.1:${HeyRuntime.PORT}/api/runtime/logs")
                    .openConnection() as HttpURLConnection
                conn.connectTimeout = 1000
                conn.readTimeout = 1000
                val t = conn.inputStream.bufferedReader().readText()
                conn.disconnect()
                t
            } catch (e: Exception) {
                "(could not fetch runtime logs: $e)"
            }
            val web = synchronized(consoleLog) { consoleLog.joinToString("\n") }
            val text = "===== WEBVIEW CONSOLE =====\n$web\n\n===== RUNTIME =====\n$runtime"
            runOnUiThread {
                val tv = TextView(this).apply {
                    setText(text)
                    textSize = 10f
                    setPadding(24, 24, 24, 24)
                    setTextIsSelectable(true)
                    typeface = android.graphics.Typeface.MONOSPACE
                }
                val scroll = ScrollView(this).apply { addView(tv) }
                AlertDialog.Builder(this)
                    .setTitle("Hey runtime logs")
                    .setView(scroll)
                    .setPositiveButton("Close", null)
                    .setNeutralButton("Copy") { _, _ ->
                        val cm = getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager
                        cm.setPrimaryClip(android.content.ClipData.newPlainText("hey-logs", text))
                    }
                    .show()
            }
        }
    }

    private fun fetchStatus(): JSONObject? = try {
        val conn = URL("http://127.0.0.1:${HeyRuntime.PORT}/api/runtime/status")
            .openConnection() as HttpURLConnection
        conn.connectTimeout = 800
        conn.readTimeout = 800
        val body = conn.inputStream.bufferedReader().readText()
        conn.disconnect()
        JSONObject(body)
    } catch (_: Exception) {
        null
    }

    private fun renderStatus(s: JSONObject?) {
        val v = statusView ?: return
        when {
            s == null || !s.optBoolean("carrier_up") -> {
                v.text = "● carrier offline"
                v.setTextColor(Color.rgb(255, 120, 120))
            }
            !s.optBoolean("online") -> {
                v.text = "● carrier starting…"
                v.setTextColor(Color.rgb(255, 210, 120))
            }
            else -> {
                val peers = s.optInt("neighbors")
                v.text = if (peers > 0) "● online · $peers peer(s)" else "● online · no peers"
                v.setTextColor(Color.rgb(120, 230, 140))
            }
        }
    }

    private fun startRuntimeService() {
        val intent = Intent(this, RuntimeService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    private fun waitForPort(port: Int, timeoutMs: Long) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            try {
                val conn = URL("http://127.0.0.1:$port/api/session").openConnection() as HttpURLConnection
                conn.connectTimeout = 500
                conn.readTimeout = 500
                conn.requestMethod = "GET"
                val code = conn.responseCode
                conn.disconnect()
                if (code in 200..499) return
            } catch (_: Exception) {
                // not up yet
            }
            Thread.sleep(150)
        }
    }

    @Deprecated("classic result API (no AndroidX activity dep)")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode == fileChooserCode) {
            val cb = filePathCallback
            filePathCallback = null
            // MUST always answer (even with null/empty) or the file input wedges.
            val result = WebChromeClient.FileChooserParams.parseResult(resultCode, data)
            Log.i("HeyWeb", "file chooser result: ${result?.size ?: 0} uri(s)")
            cb?.onReceiveValue(result ?: emptyArray())
            return
        }
        super.onActivityResult(requestCode, resultCode, data)
    }

    override fun onBackPressed() {
        if (webView.canGoBack()) webView.goBack() else super.onBackPressed()
    }
}
