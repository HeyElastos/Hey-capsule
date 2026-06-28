//! Loopback HTTP surface — the in-process replacement for `elastos-server`.
//!
//! The WASM capsule is served from `/apps/<capsule>/` and its provider/storage
//! fetches go to the SAME origin (`api_base()` resolves to ""), so hey-social
//! runs completely UNMODIFIED against this. We answer exactly the routes
//! hey-core emits and dispatch them to the in-process identity / carrier /
//! content / storage modules. Auth is a no-op: it's loopback, single-user, and
//! the identity key is already held locally.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};
use tower_http::services::{ServeDir, ServeFile};

use hey_core::identity as hid;

use crate::carrier::{err, ok, Carrier};
use crate::content::Content;
use crate::storage::Storage;

/// Per-launch bearer required on the sensitive API routes (provider / storage / session /
/// capability). Set once by the JNI init (`set_token`) BEFORE `serve()`. On Android the loopback
/// interface is shared between installed apps, so binding 127.0.0.1 is NOT a trust boundary — this
/// secret is. Empty/unset = open (the host dev harness, which never calls set_token).
static API_TOKEN: OnceLock<String> = OnceLock::new();
pub fn set_token(t: &str) {
    let _ = API_TOKEN.set(t.to_string());
}
fn authed(headers: &axum::http::HeaderMap) -> bool {
    let want = API_TOKEN.get().map(String::as_str).unwrap_or("");
    if want.is_empty() {
        // Fail closed on a device: 127.0.0.1 is shared between every installed
        // app, so an unconfigured token must never mean "open". Only the host
        // dev harness (no co-tenant threat model) runs open.
        return cfg!(not(target_os = "android"));
    }
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == want)
        .unwrap_or(false)
}

#[derive(Clone)]
pub struct AppState {
    /// Filled in asynchronously AFTER the server is already listening, so a slow
    /// or panicking carrier can never delay/prevent the port binding (which a
    /// caller sees as ERR_CONNECTION_REFUSED). Empty until the carrier is up;
    /// `peer/*` ops return "carrier starting" until then.
    pub carrier: Arc<tokio::sync::RwLock<Option<Arc<Carrier>>>>,
    /// Our public did:key — ALWAYS set, even in headless-vault boot (read from
    /// the carrier-identity blob when the seed is sealed). The seed-backed
    /// `Identity` lives in the process-global `crate::IDENTITY` OnceLock, which
    /// is empty during a headless cold start and filled by `hey_unlock`; the
    /// identity provider reads THAT (so it lights up the instant we unlock),
    /// not a boot-time snapshot.
    pub did_key: String,
    pub content: Arc<Content>,
    pub storage: Arc<Storage>,
    pub capsule: String,
}

pub async fn serve(state: AppState, dist: PathBuf, port: u16) -> anyhow::Result<()> {
    let app = router(state, dist);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    log::info!("hey-mobile-runtime listening on http://127.0.0.1:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn router(state: AppState, dist: PathBuf) -> Router {
    let capsule = state.capsule.clone();
    let index = dist.join("index.html");
    // SPA fallback: leptos_router client routes 404 on disk → serve index.html.
    let serve_dir = ServeDir::new(&dist).not_found_service(ServeFile::new(index));
    let home = format!("/apps/{capsule}/");

    Router::new()
        .route("/api/provider/:scheme/:op", post(provider))
        .route(
            "/api/apps/:capsule/storage/*path",
            get(storage_get).put(storage_put).delete(storage_delete),
        )
        .route("/api/apps/:capsule/session/start", post(session_start))
        .route("/api/apps/:capsule/runtime-token", post(session_start))
        .route("/api/session", get(session_get))
        .route("/api/runtime/status", get(runtime_status))
        .route("/api/runtime/logs", get(runtime_logs))
        .route("/api/capability/request", post(capability))
        .route("/api/capability/request/:id", get(capability))
        .route("/ipfs/*cid", get(ipfs_gateway))
        .nest_service(&format!("/apps/{capsule}"), serve_dir)
        .route("/", get(move || async move { Redirect::temporary(&home) }))
        .fallback(not_found)
        // Photos/videos arrive base64-inflated in the content/publish body;
        // axum's 2 MB default would 413 them. 64 MB headroom (media is
        // client-side compressed before upload anyway).
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .with_state(state)
}

/// Log any route the WASM app hits that we don't serve — a 404 here is a prime
/// suspect for a blank page (the app expecting an endpoint the mini-runtime
/// hasn't implemented).
async fn not_found(method: axum::http::Method, uri: axum::http::Uri) -> Response {
    log::warn!("404 {method} {uri}");
    StatusCode::NOT_FOUND.into_response()
}

// ── provider dispatch ────────────────────────────────────────────────────────

/// Identity-provider answers when the seed is SEALED (vault-ON headless boot,
/// before `hey_unlock`). `whoami` works from the persisted public did:key —
/// enough for the receiver's `ensure_session` to subscribe + form neighbors.
/// Everything that touches the seed (pubkeys / sign / x25519_dh /
/// ml_kem_decapsulate) errs cleanly; the consume path is gated off while locked,
/// so these are never reached on the hot path, and any stray caller fail-softs.
fn headless_identity(op: &str, did_key: &str) -> Value {
    match op {
        "whoami" => ok(json!({ "did_key": did_key, "principal": did_key })),
        _ => err("identity sealed — unlock Hey to use this"),
    }
}

/// Shared provider dispatch — used by BOTH the loopback HTTP handler (host dev harness) and the
/// in-process dispatcher (mobile, socket-free). Returns `None` for an unknown scheme so the caller
/// maps it to 404 (hey-core's provider_call then falls back gracefully). `blobs` rides the same
/// carrier endpoint as `peer` (blobs ALPN) for cross-device large media.
pub async fn dispatch_provider(st: &AppState, scheme: &str, op: &str, req: &Value) -> Option<Value> {
    // The capability law (guard.rs): unknown schemes keep the graceful-404
    // fallback below; a KNOWN scheme only answers its granted ops — anything
    // else is denied loudly and audited, never routed to a handler's default.
    if crate::guard::known_scheme(scheme) {
        if let Err(e) = crate::guard::check(scheme, op) {
            return Some(err(e));
        }
    } else {
        // N1 deny-by-construction: reject an unknown scheme HERE, before any handler
        // match, so adding a handler arm for a scheme that is missing from
        // CAPABILITIES can never create a served-but-unchecked path. Previously the
        // 404 was emergent (no match arm); now it is structural. All 6 served arms
        // below (identity/peer/blobs/content/ipfs/did) are in CAPABILITIES, so this
        // changes no behavior for any real call — unknown still maps to a graceful 404.
        return None;
    }
    Some(match scheme {
        // Read the LIVE process identity (filled by hey_unlock), not a boot
        // snapshot — so a headless→unlocked transition lights up immediately.
        // Sealed (headless): whoami still answers from the persisted did; ops
        // that need the seed err cleanly (callers fail-soft, never panic).
        "identity" => match crate::IDENTITY.get() {
            Some(id) => id.handle(op, req),
            None => headless_identity(op, &st.did_key),
        },
        "peer" => match st.carrier.read().await.clone() {
            Some(c) => c.handle(op, req).await,
            None => err("peer: carrier starting"),
        },
        "blobs" => match st.carrier.read().await.clone() {
            Some(c) => c.handle_blobs(op, req).await,
            None => err("blobs: carrier starting"),
        },
        "content" | "ipfs" => st.content.handle(op, req),
        "did" => did_resolve(req),
        _ => return None, // unknown scheme (e.g. hey-transcoder, not bundled)
    })
}

async fn provider(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path((scheme, op)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    log::debug!("provider {scheme}/{op}");
    match dispatch_provider(&st, &scheme, &op, &req).await {
        Some(resp) => Json(resp).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(err(format!("no provider for scheme: {scheme}"))),
        )
            .into_response(),
    }
}

fn did_resolve(req: &Value) -> Value {
    let did = req
        .get("did")
        .or_else(|| req.get("did_key"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match hid::did_key_to_public_key(did) {
        Ok(pk) => ok(json!({ "did": did, "public_key_b64": B64.encode(pk) })),
        Err(e) => err(format!("did.resolve: {e}")),
    }
}

// ── storage ──────────────────────────────────────────────────────────────────

async fn storage_get(State(st): State<AppState>, headers: axum::http::HeaderMap, Path((capsule, path)): Path<(String, String)>) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // CROSS-CAPSULE GUARD: serve only THIS server's own capsule namespace, never a foreign
    // `capsule` segment supplied in the URL (defense-in-depth on the host/loopback harness).
    if capsule != st.capsule {
        return StatusCode::FORBIDDEN.into_response();
    }
    match st.storage.get(&capsule, &path) {
        Some(s) => ([("content-type", "application/json")], s).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn storage_put(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path((capsule, path)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if capsule != st.capsule {
        return StatusCode::FORBIDDEN.into_response();
    }
    match st.storage.put(&capsule, &path, &String::from_utf8_lossy(&body)) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}
async fn storage_delete(State(st): State<AppState>, headers: axum::http::HeaderMap, Path((capsule, path)): Path<(String, String)>) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if capsule != st.capsule {
        return StatusCode::FORBIDDEN.into_response();
    }
    match st.storage.delete(&capsule, &path) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

// ── content gateway (direct <img>/<video> byte serving) ──────────────────────

async fn ipfs_gateway(State(st): State<AppState>, headers: axum::http::HeaderMap, Path(cid): Path<String>) -> Response {
    // Require the per-launch bearer like every other data route — this was the one route
    // without authed(), letting a co-resident app read locally-cached content blobs over the
    // shared loopback. The app fetches media by namespace (not this gateway), so gating it
    // breaks nothing. (DM attachments are ciphertext regardless; this closes feed/profile media.)
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Path may be "<cid>" or "<cid>/<subpath>"; we key on the leading cid.
    let cid = cid.split('/').next().unwrap_or("").to_string();
    match st.content.get_bytes(&cid) {
        Some(b) => {
            let mime = crate::content::sniff_mime(&b);
            ([("content-type", mime)], b).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ── auth no-ops (loopback, single-user, key held locally) ────────────────────

async fn session_start(headers: axum::http::HeaderMap) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "ok": true })).into_response()
}
async fn session_get(State(st): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({
        "authenticated": true,
        "did_key": st.did_key,
        "principal": st.did_key,
    }))
    .into_response()
}
async fn capability(headers: axum::http::HeaderMap) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "token": "local", "status": "granted" })).into_response()
}

/// Recent runtime log lines (ring buffer) as plain text — shown by the in-app
/// log viewer so the user can read errors on-device without adb.
async fn runtime_logs(headers: axum::http::HeaderMap) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let body = crate::logbuf::snapshot().join("\n");
    ([("content-type", "text/plain; charset=utf-8")], body).into_response()
}

/// Carrier/iroh connectivity snapshot — polled by the native status pill.
async fn runtime_status(State(st): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if !authed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match st.carrier.read().await.clone() {
        Some(c) => {
            let (v4, v6_global) = c.net_stack();
            let (pub_v6, pub_v4) = c.net_addrs();
            let (udp_v4, udp_v6) = c.udp_paths();
            let (peers, direct_peers, relay_peers) = c.conn_summary().await;
            Json(json!({
                "carrier_up": true,
                "online": c.is_online(),
                // HONEST: true only when a LIVE peer is on a direct (non-relay) path.
                // `direct_capable` keeps the old node-level reachability flag.
                "direct": direct_peers > 0,
                "direct_capable": c.is_direct(),
                "node_id": c.node_id(),
                "neighbors": peers,
                "direct_peers": direct_peers,
                "relay_peers": relay_peers,
                "ipv4": v4,
                "ipv6_global": v6_global,
                "public_v4": pub_v4,
                "public_v6": pub_v6,
                // the agnostic-chain proof + the local addrs Hey actually binds
                // (so the user can SEE a VPN split-tunnel / which interface is used)
                "udp_v4": udp_v4,
                "udp_v6": udp_v6,
                "local_addrs": c.advertised_addrs(),
            }))
            .into_response()
        }
        None => Json(json!({ "carrier_up": false, "online": false })).into_response(),
    }
}
