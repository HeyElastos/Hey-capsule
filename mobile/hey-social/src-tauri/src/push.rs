// Self-hosted ntfy push receiver (Architecture C, sovereign / no Google).
//
// The home runtime is the always-on iroh-gossip peer. When a new event arrives
// for this user it POSTs to a topic on a self-hosted ntfy server. This module
// holds a long-lived streaming connection to `{ntfy_base}/{topic}/json`, and on
// each `message` event it (1) shows a native notification and (2) emits a
// `hey://push` event into the webview so the running capsule can refresh
// immediately instead of waiting for its 5s poll.
//
// Survival across backgrounding is the foreground service's job (PushService.kt
// keeps the process alive); this task does the actual work in that kept-alive
// process. http-only on purpose: a local/LAN ntfy needs no TLS, which keeps the
// Android build free of an aws-lc/rustls cross-compile.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// Managed state: ensures only one listener task is ever spawned per process.
#[derive(Default)]
pub struct PushState {
    started: AtomicBool,
}

/// Start the ntfy listener once, if a base URL + topic are configured. Safe to
/// call from both `setup` (saved config) and `connect` (fresh config) — the
/// AtomicBool makes the second call a no-op rather than a duplicate listener.
pub fn ensure_started(app: &AppHandle, base: String, topic: String) {
    if base.trim().is_empty() || topic.trim().is_empty() {
        return;
    }
    let state = app.state::<PushState>();
    if state.started.swap(true, Ordering::SeqCst) {
        return; // already listening
    }
    let app = app.clone();
    let base = base.trim().trim_end_matches('/').to_string();
    let topic = topic.trim().to_string();
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(e) = listen_once(&app, &base, &topic).await {
                eprintln!("[hey-push] listen error: {e}; reconnecting in 3s");
            }
            // Stream ended or errored — back off, then reconnect.
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

/// Open one streaming connection and pump newline-delimited JSON messages until
/// it closes. ntfy's `/json` stream interleaves `open`/`keepalive`/`message`
/// events; we act only on `message`.
async fn listen_once(app: &AppHandle, base: &str, topic: &str) -> Result<(), String> {
    let url = format!("{base}/{topic}/json");
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("ntfy {} -> HTTP {}", url, resp.status()));
    }
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = buf.drain(..=pos).collect();
            line.pop(); // drop the '\n'
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_slice::<Value>(&line) {
                if v.get("event").and_then(Value::as_str) == Some("message") {
                    deliver(app, &v);
                }
            }
        }
    }
    Ok(())
}

fn deliver(app: &AppHandle, msg: &Value) {
    let title = msg.get("title").and_then(Value::as_str).unwrap_or("Hey");
    let body = msg.get("message").and_then(Value::as_str).unwrap_or("");
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
    // Nudge the running capsule to refresh now (it also polls every 5s).
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("hey://push", body.to_string());
    }
}
