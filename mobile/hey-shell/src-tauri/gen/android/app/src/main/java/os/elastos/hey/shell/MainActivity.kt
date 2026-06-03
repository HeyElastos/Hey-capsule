package os.elastos.hey.shell

import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // Android 13+ requires a runtime grant to post notifications.
    if (Build.VERSION.SDK_INT >= 33) {
      if (checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
          != PackageManager.PERMISSION_GRANTED) {
        requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1)
      }
    }

    // Keep the process alive so the Rust ntfy listener survives backgrounding.
    PushService.start(this)
  }
}
