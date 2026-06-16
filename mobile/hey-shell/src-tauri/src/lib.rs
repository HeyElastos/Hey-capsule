// Hey shell — the native side of the "remote window" Android/desktop app.
//
// The whole app is deliberately tiny: the launcher webview (../src) collects a
// runtime host + launch token + which capsule to open, then `connect` navigates
// the SAME webview to `https://<host>/apps/<capsule>/?home_token=<token>`. From
// that point the real Hey Social / Hey Chat WASM runs unmodified against its
// serving origin — every provider call, the session cookie, Carrier/content/DID
// all stay on the home runtime. The phone holds no keys and no P2P node.
//
// `useHttpsScheme = true` (tauri.conf.json) makes the local launcher run on an
// https:// origin too, so navigating to the remote https:// origin keeps the
// WebView's cookie/localStorage jar intact instead of resetting it.
//
// Architecture C (hybrid) adds a self-hosted ntfy push receiver (see push.rs):
// the home runtime wakes this app on a new event so the messenger is live in the
// background instead of only while foregrounded.

mod push;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{Manager, Url};

#[derive(Serialize, Deserialize, Default, Clone)]
struct Config {
    #[serde(default)]
    host: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    app: String,
    /// Self-hosted ntfy base URL, e.g. http://192.168.1.10:2587 (optional).
    #[serde(default)]
    ntfy_url: String,
    /// ntfy topic this device subscribes to for push (optional).
    #[serde(default)]
    ntfy_topic: String,
}

/// Where we remember the last successful connection (best-effort).
fn config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("hey-shell.json"))
}

fn read_config(app: &tauri::AppHandle) -> Config {
    config_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Prefill the launcher fields from the last connect.
#[tauri::command]
fn load_config(app: tauri::AppHandle) -> Config {
    read_config(&app)
}

/// Build `https://<host>/apps/<capsule>/?home_token=<token>`, tolerating a host
/// given with or without a scheme and with a trailing slash.
fn build_remote_url(host: &str, app_name: &str, token: &str) -> Result<Url, String> {
    let host = host.trim().trim_end_matches('/');
    if host.is_empty() {
        return Err("Runtime host is required".into());
    }
    let base = if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("https://{host}")
    };
    // Only ever open a known capsule path.
    let capsule = match app_name {
        "hey-chat" => "hey-chat",
        _ => "hey-social",
    };
    let mut url = format!("{base}/apps/{capsule}/");
    let token = token.trim();
    if !token.is_empty() {
        url.push_str("?home_token=");
        url.push_str(&urlencoding::encode(token));
    }
    Url::parse(&url).map_err(|e| format!("Bad URL: {e}"))
}

/// Persist the inputs, start the push listener, and navigate to the remote capsule.
#[tauri::command]
fn connect(
    app: tauri::AppHandle,
    host: String,
    token: String,
    app_name: String,
    ntfy_url: Option<String>,
    ntfy_topic: Option<String>,
) -> Result<(), String> {
    let url = build_remote_url(&host, &app_name, &token)?;
    let ntfy_url = ntfy_url.unwrap_or_default();
    let ntfy_topic = ntfy_topic.unwrap_or_default();
    if let Some(p) = config_path(&app) {
        let cfg = Config {
            host,
            token,
            app: app_name,
            ntfy_url: ntfy_url.clone(),
            ntfy_topic: ntfy_topic.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = fs::write(p, json);
        }
    }
    // Begin receiving push for this device (no-op if already listening or unset).
    push::ensure_started(&app, ntfy_url, ntfy_topic);

    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    win.navigate(url).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_notification::init());
    // Camera QR scanner is mobile-only.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    builder
        .manage(push::PushState::default())
        .invoke_handler(tauri::generate_handler![load_config, connect])
        .setup(|app| {
            // If a previous session configured push, start listening at boot so
            // notifications arrive even before the user taps Connect again.
            let cfg = read_config(app.handle());
            push::ensure_started(app.handle(), cfg.ntfy_url, cfg.ntfy_topic);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Hey shell");
}
