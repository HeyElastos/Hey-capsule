// Runtime HTTP client — Rust port of capsules/hey-social/client/src/lib/runtime.js.
//
// Minimal slice needed for passkey sign-in: api_url(), runtime-token bearer
// exchange (patch 0001), fetch wrapper with credentials + bearer header, and
// the storage shape dispatcher used by the shared-identity dual-write.
//
// The rest of the runtime surface (peer/ipfs/did/elacity provider calls,
// per-capsule namespaced storage with capability tokens) is still TODO —
// stubbed below so the public module shape stays compatible with the JS
// version as more sign-in-adjacent flows get ported.

#![allow(dead_code)]

use gloo_storage::{SessionStorage, Storage as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestCredentials, RequestInit, Response};

pub const CAPSULE_ID: &str = "hey-social-rust";

const RUNTIME_TOKEN_KEY: &str = "hey-runtime-token";
const HOME_LAUNCH_TOKEN_KEY: &str = "hey-home-launch-token";
const ROUTE_MODE_KEY: &str = "hey-storage-route-mode";

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub status: Option<u16>,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
        }
    }
    pub fn with_status(message: impl Into<String>, status: u16) -> Self {
        Self {
            message: message.into(),
            status: Some(status),
        }
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(s) => write!(f, "{} (HTTP {})", self.message, s),
            None => write!(f, "{}", self.message),
        }
    }
}

fn window() -> web_sys::Window {
    web_sys::window().expect("no window")
}

// Derive the install base ("/elastos" under YunoHost subpath, "" at root)
// from the iframe's URL. Same regex as the JS shell-core.
pub fn api_base() -> String {
    let path = window().location().pathname().unwrap_or_default();
    // Match (.*?)/apps/<name>/ — capture everything before "/apps/".
    if let Some(idx) = path.find("/apps/") {
        return path[..idx].to_string();
    }
    String::new()
}

pub fn api_url(path: &str) -> String {
    format!("{}{}", api_base(), path)
}

// Read ?home_token / ?runtime_token off the launch URL, cache it in
// sessionStorage. Same dual-name handling as the JS version (v0.3 sends
// home_token; legacy v0.2 builds sent runtime_token).
pub fn home_launch_token() -> Option<String> {
    if let Ok(Some(v)) = SessionStorage::get::<Option<String>>(HOME_LAUNCH_TOKEN_KEY) {
        // Re-check the URL — a new launch envelope means a runtime restart.
        let url_tok = read_url_token();
        if let Some(fresh) = url_tok.as_ref() {
            if Some(fresh) != Some(&v) {
                // Drop caches bound to the previous session.
                let _ = SessionStorage::delete(RUNTIME_TOKEN_KEY);
                let _ = SessionStorage::set(HOME_LAUNCH_TOKEN_KEY, fresh);
                return Some(fresh.clone());
            }
        }
        return Some(v);
    }
    if let Some(fresh) = read_url_token() {
        let _ = SessionStorage::set(HOME_LAUNCH_TOKEN_KEY, &fresh);
        return Some(fresh);
    }
    None
}

fn read_url_token() -> Option<String> {
    let search = window().location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params
        .get("home_token")
        .or_else(|| params.get("runtime_token"))
}

// Exchange home-launch envelope → session bearer via patch 0001's
// /api/apps/:capsule/runtime-token endpoint. Cached in sessionStorage.
// Resolves to true on success, false on any failure (caller decides).
pub async fn bearer_ready() -> bool {
    if let Ok(Some(_existing)) = SessionStorage::get::<Option<String>>(RUNTIME_TOKEN_KEY) {
        return true;
    }
    let Some(launch) = home_launch_token() else {
        return false;
    };
    let url = api_url(&format!("/api/apps/{CAPSULE_ID}/runtime-token"));
    let headers = serde_json::json!({
        "Content-Type": "application/json",
        "x-elastos-home-token": launch,
    });
    match fetch_raw(&url, "POST", Some("{}".to_string()), &headers).await {
        Ok(resp) => {
            if !resp.ok() {
                log_warn(&format!(
                    "[hey-social-rust] runtime-token exchange failed: {}",
                    resp.status()
                ));
                return false;
            }
            match JsFuture::from(resp.json().unwrap()).await {
                Ok(v) => {
                    let json: Value = serde_wasm_bindgen::from_value(v).unwrap_or(Value::Null);
                    if let Some(tok) = json.get("token").and_then(|t| t.as_str()) {
                        let _ = SessionStorage::set(RUNTIME_TOKEN_KEY, tok);
                        return true;
                    }
                    false
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

fn current_runtime_token() -> Option<String> {
    SessionStorage::get::<Option<String>>(RUNTIME_TOKEN_KEY)
        .ok()
        .flatten()
}

// Low-level fetch — builds a Request from method + headers + body, awaits
// the global fetch, returns the Response. credentials: "include" is forced
// so the runtime's session cookie travels with every call.
async fn fetch_raw(
    url: &str,
    method: &str,
    body: Option<String>,
    headers: &Value,
) -> Result<Response, JsValue> {
    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_credentials(RequestCredentials::Include);
    if let Some(b) = body.as_deref() {
        opts.set_body(&JsValue::from_str(b));
    }
    let req = Request::new_with_str_and_init(url, &opts)?;
    let hdrs = req.headers();
    if let Some(map) = headers.as_object() {
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                hdrs.set(k, s)?;
            }
        }
    }
    let resp_value = JsFuture::from(window().fetch_with_request(&req)).await?;
    resp_value.dyn_into::<Response>()
}

use wasm_bindgen::JsCast;

// Public helper used by passkey.rs to call upstream's /api/auth/passkey/*
// endpoints. Always carries the session cookie; carries the bearer header
// once bearer_ready() has resolved (idempotent — safe to call on every hit).
pub async fn upstream_fetch(
    path: &str,
    method: &str,
    body: Option<String>,
) -> Result<Response, RuntimeError> {
    let _ = bearer_ready().await;
    let mut headers = serde_json::json!({ "Content-Type": "application/json" });
    if let Some(tok) = current_runtime_token() {
        headers["Authorization"] = Value::String(format!("Bearer {tok}"));
    }
    let url = api_url(path);
    fetch_raw(&url, method, body, &headers)
        .await
        .map_err(|e| RuntimeError::new(format!("fetch error: {e:?}")))
}

// ── Shared storage (cross-capsule .AppData/* paths) ───────────────────
//
// Used by passkey.rs at the end of sign-in to dual-write the shared
// identity profile so other Hey capsules see this user as signed up.
// Try patch-0002 (POST /api/apps/:capsule/storage/<suffix>) first, fall
// back to /api/localhost/Users/self/<suffix>. Memoize the working shape
// in sessionStorage so subsequent writes skip the probe.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedIdentity {
    pub name: String,
    #[serde(rename = "didKey")]
    pub did_key: String,
    #[serde(rename = "recoveryKeyHash")]
    pub recovery_key_hash: String,
    pub passkeys: Vec<Value>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "createdBy")]
    pub created_by: String,
}

fn route_mode() -> Option<String> {
    SessionStorage::get::<Option<String>>(ROUTE_MODE_KEY)
        .ok()
        .flatten()
}

fn set_route_mode(mode: &str) {
    let _ = SessionStorage::set(ROUTE_MODE_KEY, mode);
}

fn build_storage_url(mode: &str, suffix: &str) -> (String, Value) {
    let suffix = suffix.trim_start_matches('/');
    if mode == "patch-0002" {
        let url = format!("{}/api/apps/{}/storage/{}", api_base(), CAPSULE_ID, suffix);
        let headers = if let Some(launch) = home_launch_token() {
            serde_json::json!({ "x-elastos-home-token": launch })
        } else {
            Value::Null
        };
        return (url, headers);
    }
    let url = format!("{}/api/localhost/Users/self/{}", api_base(), suffix);
    let headers = if let Some(tok) = current_runtime_token() {
        serde_json::json!({ "Authorization": format!("Bearer {tok}") })
    } else {
        Value::Null
    };
    (url, headers)
}

async fn shared_dispatch(
    suffix: &str,
    method: &str,
    body: Option<String>,
) -> Result<Response, RuntimeError> {
    let _ = bearer_ready().await;
    let attempt = |mode: String,
                   suffix: String,
                   method: String,
                   body: Option<String>| async move {
        let (url, mut headers) = build_storage_url(&mode, &suffix);
        if body.is_some() {
            headers["Content-Type"] = Value::String("application/json".into());
        }
        fetch_raw(&url, &method, body, &headers).await
    };

    if let Some(mode) = route_mode() {
        return attempt(mode, suffix.into(), method.into(), body)
            .await
            .map_err(|e| RuntimeError::new(format!("storage fetch: {e:?}")));
    }
    // Probe patch-0002 first; fall back to legacy on 401/403/404.
    let resp = attempt(
        "patch-0002".into(),
        suffix.into(),
        method.into(),
        body.clone(),
    )
    .await
    .map_err(|e| RuntimeError::new(format!("storage fetch: {e:?}")))?;
    let status = resp.status();
    if status == 401 || status == 403 || status == 404 {
        let legacy = attempt("legacy".into(), suffix.into(), method.into(), body)
            .await
            .map_err(|e| RuntimeError::new(format!("storage fetch: {e:?}")))?;
        let ls = legacy.status();
        if ls < 500 && ls != 401 && ls != 403 {
            set_route_mode("legacy");
            return Ok(legacy);
        }
        set_route_mode("patch-0002");
        return Ok(resp);
    }
    set_route_mode("patch-0002");
    Ok(resp)
}

pub async fn shared_write_json(suffix: &str, value: &Value) -> Result<(), RuntimeError> {
    let body = serde_json::to_string(value)
        .map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    let resp = shared_dispatch(suffix, "PUT", Some(body)).await?;
    if !resp.ok() {
        return Err(RuntimeError::with_status(
            format!("shared_write_json PUT {suffix}"),
            resp.status(),
        ));
    }
    Ok(())
}

fn log_warn(s: &str) {
    web_sys::console::warn_1(&JsValue::from_str(s));
}

// ── Placeholder namespaces — fill in as more flows get ported ─────────

pub mod storage {
    // localhost:// CRUD per-capsule namespace — TODO.
}

pub mod peer {
    // elastos://peer/hey-v0/* — Carrier gossip via provider bus — TODO.
}

pub mod ipfs {
    // elastos://ipfs/* — Kubo-backed media storage — TODO.
}

pub mod did_provider {
    // elastos://did/* — DID resolution — TODO.
}

pub mod elacity {
    // elastos://elacity/* — Elacity Player capsule for DASH/CENC playback.
    // See reference_elacity_player memory for integration shape — TODO.
}
