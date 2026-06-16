//! Best-effort cross-platform OS desktop notifications (the desktop parity for
//! Android's `RuntimeService.notifyEvent`). Backed by `notify-rust` (libnotify/dbus
//! on Linux, the native APIs on macOS/Windows).
//!
//! Every call is strictly best-effort: a failure (no notification daemon, a denied
//! permission, a closed dbus session) is logged at debug and swallowed — it must
//! never panic or block the UI thread. Mirrors Android's `runCatching { notify() }`.

/// Post one OS notification with `title` + `body`. Failures are ignored.
///
/// `summary` is required by the freedesktop spec; an empty title falls back to
/// "Hey" so the bubble is never blank (matches Android's `ifBlank { "Hey" }`).
pub fn post(title: &str, body: &str) {
    let summary = if title.trim().is_empty() { "Hey" } else { title };
    let res = notify_rust::Notification::new()
        .appname("Hey")
        .summary(summary)
        .body(body)
        .show();
    if let Err(e) = res {
        // Never surface this to the user — a missing notification daemon is a
        // perfectly normal headless/CI environment, not an app error.
        log::debug!("os notification suppressed: {e}");
    }
}
