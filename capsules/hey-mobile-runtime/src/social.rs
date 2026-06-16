//! Native Hey Social app-API — the high-level operations the Compose UI calls
//! via JNI. Everything routes through hey-core (so crypto/identity/transport are
//! byte-identical to the web + CLI builds); this module only assembles posts/
//! feed/profile on top of hey-core's content + storage + identity + events.
//!
//! v1 post format is a signed JSON event stored in the on-device runtime
//! (self-consistent: your posts show in your feed, media renders from the local
//! /ipfs gateway). Cross-CLIENT interop with the web app's dag-cbor `post.create.v2`
//! is the next step (lift hey-social `ipld.rs`/`posts.rs` into a shared rlib) —
//! tracked, deliberately deferred so the native photo-post loop lands first.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use base64::engine::general_purpose::STANDARD as B64S;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64U;
use base64::Engine as _;
use serde_json::{json, Value};

use hey_core::api::dms;
use hey_core::events::{self, VerifyResult};
use hey_core::runtime::{self, content, identity_provider, shared_read_json, shared_write_json};
use hey_core::session;

const NS: &str = "hey"; // shared identity namespace (must equal hey-core HEY_NAMESPACE)
const FEED_INDEX: &str = "hey-social/native-feed.json";
const RECV_CONSUMER: &str = "hey-social-recv";

/// hey-social's capsule context. hey-core reads it via THREAD-LOCAL accessors,
/// so `install_ctx()` must run on every thread that touches the engine (each
/// JNI call thread + the receiver thread) — same discipline as plat::set_base.
pub const HEY_SOCIAL_CTX: hey_core::ctx::CapsuleCtx = hey_core::ctx::CapsuleCtx {
    capsule_id: "hey-social",
    private_namespace: "Hey",
    session_key: "hey-social-session",
    welcomed_key: "hey-social-welcomed",
    session_redeemed_key: "hey-session-redeemed",
    home_launch_token_key: "hey-home-launch-token",
    runtime_token_key: "hey-runtime-token",
    token_store_key: "hey-capability-tokens",
    route_mode_key: "hey-storage-route-mode",
    boot_capabilities: &[
        ("elastos://peer/*", "message"),
        ("elastos://content/*", "write"),
        ("elastos://did/*", "read"),
    ],
};

/// Install the per-capsule context on the current thread (idempotent per thread).
pub fn install_ctx() {
    hey_core::ctx::init(HEY_SOCIAL_CTX);
}

/// Monotonic counter bumped whenever the receiver ingests something new from the
/// carrier (post / like / comment / media). The UI polls this and reloads only
/// when it changes — auto-refresh without the user touching anything.
fn rev_cell() -> &'static AtomicU64 {
    static R: OnceLock<AtomicU64> = OnceLock::new();
    R.get_or_init(|| AtomicU64::new(0))
}
pub fn feed_rev() -> u64 {
    rev_cell().load(Ordering::Relaxed)
}
fn bump_rev() {
    rev_cell().fetch_add(1, Ordering::Relaxed);
}

/// Pending local-notification events, filled by the receiver when it ingests
/// something noteworthy and drained by the foreground service (which posts them
/// as Android notifications — no Firebase, GrapheneOS-friendly).
fn notif_queue() -> &'static Mutex<std::collections::VecDeque<Value>> {
    static Q: OnceLock<Mutex<std::collections::VecDeque<Value>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(std::collections::VecDeque::new()))
}
/// `key` is a per-event discriminator (post id, follower did, …) so the Android
/// side can give distinct events distinct notification ids instead of collapsing
/// every "mention from Alice" onto one entry that overwrites the last.
fn push_notif(kind: &str, title: &str, body: &str, did: &str, key: &str) {
    if let Ok(mut q) = notif_queue().lock() {
        if q.len() >= 50 {
            q.pop_front();
        }
        q.push_back(json!({ "kind": kind, "title": title, "body": body, "did": did, "key": key }));
    }
}
/// Drain + clear pending notifications (called by the foreground service).
pub fn drain_notifs() -> Value {
    let items: Vec<Value> = notif_queue().lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
    json!(items)
}

/// Per-author gossip topic. A follower joins the author's topic to receive their
/// posts; reactions/comments on a post are published on the POST AUTHOR's topic.
fn feed_topic(did: &str) -> String {
    format!("hey-social/feed/{did}")
}

/// Install a minimal provider-backed session (did:key only) so
/// `events::create_signed_event` can sign. File-backed, so it persists across
/// the per-call JNI threads once written.
pub async fn ensure_session() -> Result<String, String> {
    match whoami_did().await {
        Ok(did) => {
            // Reconcile the (persisted, file-backed) session against the
            // AUTHORITATIVE identity. A session left over from a PRIOR identity
            // (did_key diverged from whoami) is catastrophic: every signed event
            // and every DETERMINISTIC per-pair DM queue is derived under the wrong
            // DID, so the peer sends to pair(my_real_did, them) while we listen on
            // pair(my_stale_did, them) — and EVERY inbound DM silently misses.
            // Always re-point the session at whoami when they differ.
            if let Some(s) = session::current() {
                if s.did_key == did {
                    return Ok(did);
                }
                log::warn!(
                    "session did_key {} != identity {} — resetting session to identity",
                    s.did_key, did
                );
            }
            session::set(&session::Session {
                auth_key_hex: String::new(),
                did_key: did.clone(),
                name: String::new(),
                ml_kem_secret_b64: String::new(),
                ml_kem_public_b64: String::new(),
            });
            Ok(did)
        }
        // Identity not available yet (e.g. sealed/headless before unlock): fall
        // back to a valid persisted session so headless operation continues.
        Err(e) => session::current()
            .map(|s| s.did_key)
            .filter(|d| d.starts_with("did:key:z"))
            .ok_or(e),
    }
}

/// Sign an event and broadcast it on a gossip topic. Local state is durable and
/// followers backfill (`sync_req`), so live gossip is the push half of delivery —
/// but a transport failure must be LOUD AND CLOSED, never a silent no-op: any
/// failure is queued (bounded, retried by `poll_once`), counted in
/// `carrier_health.pending_broadcasts`, and logged. Returns true only when the
/// event was actually handed to the carrier.
async fn publish(topic: &str, event_type: &str, payload: Value) -> bool {
    match try_publish(topic, event_type, &payload).await {
        Ok(()) => true,
        Err(e) => {
            log::warn!("publish {event_type} on {topic} failed ({e}); queued for retry");
            queue_broadcast(topic, event_type, payload);
            false
        }
    }
}

async fn try_publish(topic: &str, event_type: &str, payload: &Value) -> Result<(), String> {
    ensure_session().await?;
    let ev = events::create_signed_event(event_type, payload.clone())
        .await
        .map_err(|e| format!("sign: {e}"))?;
    let wire = events::to_wire_string(&ev);
    let resp = runtime::peer::publish(runtime::peer::PublishArgs {
        topic,
        message: &wire,
        sender_id: &ev.sender_did,
        ts: ev.ts,
        signature: &ev.signature,
    })
    .await
    .map_err(|e| format!("publish: {e}"))?;
    // `broadcast:"local_only"` = the carrier's broadcast itself failed (honest
    // delivery contract, see carrier.rs gossip_send) — that's a retryable
    // transport failure. 0 neighbors with a clean broadcast is NOT a failure:
    // offline followers catch up via backfill (sync_req).
    let local_only = resp
        .get("broadcast")
        .or_else(|| resp.get("data").and_then(|d| d.get("broadcast")))
        .and_then(Value::as_str)
        == Some("local_only");
    if local_only {
        return Err("carrier broadcast failed (local_only)".into());
    }
    Ok(())
}

/// Failed social broadcasts awaiting retry: (topic, event_type, payload, attempts).
/// In-memory only — the posts/reactions themselves are durable in storage and
/// followers backfill, so a process death loses just the live-push optimization.
fn pending_queue() -> &'static Mutex<std::collections::VecDeque<(String, String, Value, u32)>> {
    static Q: OnceLock<Mutex<std::collections::VecDeque<(String, String, Value, u32)>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(std::collections::VecDeque::new()))
}

const PENDING_BROADCAST_CAP: usize = 256;
const PENDING_BROADCAST_ATTEMPTS: u32 = 30;

fn queue_broadcast(topic: &str, event_type: &str, payload: Value) {
    if let Ok(mut q) = pending_queue().lock() {
        if q.len() >= PENDING_BROADCAST_CAP {
            // Loud overflow: dropping the OLDEST keeps the queue honest about
            // recency; the drop itself is recorded, never silent.
            if let Some((t, e, _, _)) = q.pop_front() {
                crate::guard::audit("broadcast.drop", json!({ "topic": t, "event": e, "reason": "queue full" }));
            }
        }
        q.push_back((topic.to_string(), event_type.to_string(), payload, 0));
    }
}

/// How many social broadcasts are still waiting to reach the carrier.
pub fn pending_broadcasts() -> usize {
    pending_queue().lock().map(|q| q.len()).unwrap_or(0)
}

/// Retry queued broadcasts (called from the receiver loop each poll cycle).
/// Re-signs on each attempt; an item that keeps failing is dropped LOUDLY
/// (warn + audit) after PENDING_BROADCAST_ATTEMPTS.
async fn flush_pending_broadcasts() {
    let batch: Vec<(String, String, Value, u32)> = match pending_queue().lock() {
        Ok(mut q) => q.drain(..).collect(),
        Err(_) => return,
    };
    for (topic, event_type, payload, attempts) in batch {
        if try_publish(&topic, &event_type, &payload).await.is_ok() {
            continue;
        }
        let attempts = attempts + 1;
        if attempts >= PENDING_BROADCAST_ATTEMPTS {
            log::warn!("broadcast {event_type} on {topic} dropped after {attempts} attempts");
            crate::guard::audit(
                "broadcast.drop",
                json!({ "topic": topic, "event": event_type, "reason": format!("{attempts} attempts") }),
            );
            continue;
        }
        if let Ok(mut q) = pending_queue().lock() {
            q.push_back((topic, event_type, payload, attempts));
        }
    }
}

/// Serializes read-modify-write of shared JSON (feed index, reactions, comments)
/// across the JNI call threads + the receiver threads, so concurrent local +
/// ingested mutations don't lose each other (last-writer-wins) or corrupt the
/// index. Coarse but correct + cheap for local file I/O.
fn storage_lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn add_to_index(id: &str) {
    let _g = storage_lock().lock().await;
    let mut idx: Vec<Value> = shared_read_json(FEED_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    if !idx.iter().any(|v| v.as_str() == Some(id)) {
        idx.insert(0, json!(id));
        let _ = shared_write_json(FEED_INDEX, &json!(idx)).await;
    }
}

fn err<T>(m: impl Into<String>) -> Result<T, String> {
    Err(m.into())
}

/// Current user's did:key (runtime-held identity).
pub async fn whoami_did() -> Result<String, String> {
    let resp = identity_provider::whoami(NS).await.map_err(|e| format!("whoami: {e}"))?;
    let d = resp.get("data").unwrap_or(&resp);
    d.get("did_key")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| "whoami: no did_key".into())
}

/// {did, ticket} for the identity gate + invite links.
pub async fn whoami() -> Result<Value, String> {
    let did = whoami_did().await?;
    let ticket = runtime::peer::my_ticket().await.unwrap_or_default();
    Ok(json!({ "did": did, "ticket": ticket }))
}

/// {online, node_id, peer_count} for the connection badge. Uses the runtime's
/// /api/runtime/status, whose `online` is the load-bearing relay-connected
/// signal (is_online) — NOT merely "the endpoint bound". Falls back to the
/// peer-op health if the status route is unavailable.
pub async fn carrier_health() -> Value {
    let url = format!("{}/api/runtime/status", runtime::api_base());
    if let Ok((_, body)) = hey_core::plat::http("GET", &url, None) {
        if let Ok(v) = serde_json::from_str::<Value>(&body) {
            return json!({
                "online": v.get("online").and_then(Value::as_bool).unwrap_or(false),
                "direct": v.get("direct").and_then(Value::as_bool).unwrap_or(false),
                "direct_capable": v.get("direct_capable").and_then(Value::as_bool).unwrap_or(false),
                "direct_peers": v.get("direct_peers").and_then(Value::as_i64).unwrap_or(0),
                "relay_peers": v.get("relay_peers").and_then(Value::as_i64).unwrap_or(0),
                "node_id": v.get("node_id").and_then(Value::as_str).unwrap_or(""),
                "peer_count": v.get("neighbors").and_then(Value::as_i64).unwrap_or(0),
                "ipv4": v.get("ipv4").and_then(Value::as_bool).unwrap_or(false),
                "ipv6_global": v.get("ipv6_global").and_then(Value::as_bool).unwrap_or(false),
                "public_v4": v.get("public_v4").cloned().unwrap_or(Value::Null),
                "public_v6": v.get("public_v6").cloned().unwrap_or(Value::Null),
                "udp_v4": v.get("udp_v4").and_then(Value::as_bool).unwrap_or(false),
                "udp_v6": v.get("udp_v6").and_then(Value::as_bool).unwrap_or(false),
                "local_addrs": v.get("local_addrs").cloned().unwrap_or(Value::Array(vec![])),
                "pending_broadcasts": pending_broadcasts(),
            });
        }
    }
    let h = runtime::peer::carrier_health().await;
    json!({
        "online": h.online,
        "node_id": h.node_id,
        "peer_count": h.peer_count,
        "pending_broadcasts": pending_broadcasts(),
    })
}

/// Resolve a content CID to bytes via the canonical content provider — the
/// backend for the `elastos://<cid>` namespace. The UI references media by
/// namespace (never an IP/gateway); this hides the network behind the runtime.
pub async fn content_bytes(cid: &str) -> Vec<u8> {
    if cid.is_empty() {
        return Vec::new();
    }
    content::get_bytes(cid, None).await.unwrap_or_default()
}

/// Pin media bytes to the on-device content store. Returns a media tile.
pub async fn upload_media(bytes: &[u8], mime: &str, filename: &str) -> Result<Value, String> {
    let resp = content::add_bytes(bytes, filename, true)
        .await
        .map_err(|e| format!("content.publish: {e}"))?;
    let cid = content::extract_cid(&resp).ok_or("content.publish: no CID")?;
    let kind = if mime.starts_with("video/") { "video" } else { "photo" };
    Ok(json!({ "cid": cid, "mime": mime, "type": kind, "name": filename }))
}

/// Create a post from a caption + already-uploaded media tiles (JSON array).
pub async fn create_post(caption: &str, media_tiles_json: &str) -> Result<Value, String> {
    let did = whoami_did().await?;
    let media: Value = serde_json::from_str(media_tiles_json).unwrap_or_else(|_| json!([]));
    let id = new_id();
    let ts = hey_core::plat::now_ms();
    let prof = my_profile().await;
    // Denormalize author name + avatar so the feed shows them without a lookup.
    let post = json!({
        "id": id,
        "type": "post.create",
        "author": did,
        "author_name": prof.get("nickname").and_then(Value::as_str).unwrap_or(""),
        "author_avatar": prof.get("avatar").and_then(Value::as_str).unwrap_or(""),
        "caption": caption,
        "media": media,
        "ts": ts,
    });
    // Store the post + index it, then broadcast it on my author topic so
    // followers receive it over the carrier.
    shared_write_json(&format!("hey-social/posts/{id}.json"), &post)
        .await
        .map_err(|e| format!("store post: {e}"))?;
    add_to_index(&id).await;
    publish(&feed_topic(&did), "hey-social.post", post.clone()).await;
    Ok(post)
}

/// My profile (nickname/bio/avatar), or empty defaults.
async fn my_profile() -> Value {
    shared_read_json("hey-social/profile.json").await.ok().flatten().unwrap_or_else(|| json!({}))
}

/// Set/update my profile, persist it, and broadcast it so followers re-cache.
pub async fn set_profile(nickname: &str, bio: &str, avatar: &str) -> Result<Value, String> {
    let did = whoami_did().await?;
    // Preserve any published tip addresses across a profile edit.
    let addresses = my_profile().await.get("addresses").cloned().unwrap_or(Value::Null);
    let p = json!({ "did": did, "nickname": nickname, "bio": bio, "avatar": avatar, "addresses": addresses, "ts": hey_core::plat::now_ms() });
    shared_write_json("hey-social/profile.json", &p).await.map_err(|e| e.to_string())?;
    broadcast_profile(&did, &p).await;
    Ok(p)
}

/// Publish my tip-receive addresses (a `{chainKey: address}` map) INSIDE my signed
/// profile, so followers can tip me by identity — they never need my address. The
/// Ed25519 signature that authenticates my posts authenticates these addresses too.
pub async fn set_tip_addresses(addresses_json: &str) -> Result<Value, String> {
    let did = whoami_did().await?;
    let addresses: Value = serde_json::from_str(addresses_json).unwrap_or(Value::Null);
    let mut p = my_profile().await;
    if !p.is_object() {
        p = json!({});
    }
    p["did"] = json!(did);
    p["addresses"] = addresses;
    p["ts"] = json!(hey_core::plat::now_ms());
    shared_write_json("hey-social/profile.json", &p).await.map_err(|e| e.to_string())?;
    broadcast_profile(&did, &p).await;
    Ok(p)
}

/// Broadcast the full profile (incl. tip addresses) so followers re-cache it.
async fn broadcast_profile(did: &str, p: &Value) {
    publish(
        &feed_topic(did),
        "hey-social.profile",
        json!({
            "nickname": p.get("nickname").and_then(Value::as_str).unwrap_or(""),
            "bio": p.get("bio").and_then(Value::as_str).unwrap_or(""),
            "avatar": p.get("avatar").and_then(Value::as_str).unwrap_or(""),
            "addresses": p.get("addresses").cloned().unwrap_or(Value::Null),
        }),
    )
    .await;
}

/// The tip-receive addresses a peer has published (`{chainKey: address}`), or null.
pub async fn resolve_tip(did: &str) -> Value {
    get_profile(did).await.get("addresses").cloned().unwrap_or(Value::Null)
}

/// Tell a tip RECIPIENT they were tipped — over the PRIVATE end-to-end DM channel,
/// NOT a public broadcast. The amount is encrypted to them and authenticated as
/// coming from a real contact (no spoofing, no leak to their followers), and it
/// rides the DM outbox so it's offline-queued + retried. Surfaces in the chat
/// thread + a message notification even with the app closed. Returns whether the
/// notice was delivered to the DM layer.
///
/// This only reaches people you have an established chat with (every chat-tip, and
/// any creator you've messaged). Tipping a follow-only creator still lands the
/// on-chain transfer — they'll see it in their wallet — but gets no private notice,
/// because there's no confidential channel to a non-contact. `txid` is reserved
/// for a future on-chain-verified notice. Public per-post "superchat" tips, if
/// ever wanted, must be an explicit opt-in (they expose the amount to followers).
pub async fn notify_tip(to_did: &str, sym: &str, amount: &str, _txid: &str) -> bool {
    if to_did.is_empty() {
        return false;
    }
    let amount = amount.trim();
    let sym = sym.trim();
    let body = if !amount.is_empty() && !sym.is_empty() {
        format!("💰 Sent you a tip of {amount} {sym}")
    } else {
        "💰 Sent you a tip".to_string()
    };
    chat_send(to_did, &body).await.is_ok()
}

/// Hidden DM control message carrying a contact's tip-receive addresses, so two
/// people who can DM each other can ALWAYS tip by identity — even without following
/// (the feed/profile path needs a follow + feed ticket; a chat-only contact has
/// neither). SOH-prefixed so it's never user-typed; cached + stripped from the
/// thread on read (see chat_conversation).
const ADDR_PREFIX: &str = "\u{1}hey-addr:1:";
/// Hidden 1:1 voice-call control messages (offer / accept / decline / end), base64(json) payload,
/// over the SAME SOH-prefixed E2E-DM channel as the address card — stripped from the visible thread.
/// The native CallManager drives ringing + call state off `call_poll()`.
const CALL_PREFIX: &str = "\u{1}hey-call:1:";
/// Hidden "delete this message" tombstone: base64(json {"id": <msg id>}) over the SAME E2E channel.
/// A reader hides a message only when a tombstone with the SAME `mine`-side references its id — so
/// you can only delete your OWN messages (yours are `mine` to you, and arrive from the same single
/// sender to the peer). Stripped from the visible thread.
const DEL_PREFIX: &str = "\u{1}hey-del:1:";

/// (id, mine) pairs tombstoned for deletion within a conversation slice.
fn collect_deleted(arr: &[Value]) -> std::collections::HashSet<(String, bool)> {
    let mut set = std::collections::HashSet::new();
    for m in arr {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let Some(b64) = text.strip_prefix(DEL_PREFIX) else { continue };
        let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
        if let Ok(bytes) = B64U.decode(b64.trim()) {
            if let Ok(p) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(id) = p.get("id").and_then(Value::as_str) {
                    set.insert((id.to_string(), mine));
                }
            }
        }
    }
    set
}

/// Delete one of MY messages for everyone: post a tombstone referencing its id over the chat's E2E
/// channel. Readers (incl. me) hide the referenced message. No-op on empty ids.
pub async fn delete_chat_message(chat_id: &str, msg_id: &str, is_group: bool) -> bool {
    if chat_id.is_empty() || msg_id.is_empty() {
        return false;
    }
    let b64 = B64U.encode(json!({ "id": msg_id }).to_string().as_bytes());
    let payload = format!("{DEL_PREFIX}{b64}");
    if is_group {
        dms::send_group_message(chat_id, &payload).await.is_ok()
    } else {
        chat_send(chat_id, &payload).await.is_ok()
    }
}

/// Edits work like deletes: a hidden mutation referencing the target id; readers
/// replace the text in place and tag it edited. Latest edit wins.
const EDIT_PREFIX: &str = "\u{1}hey-edit:1:";

fn collect_edits(arr: &[Value]) -> std::collections::HashMap<(String, bool), String> {
    let mut map = std::collections::HashMap::new();
    for m in arr {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let Some(b64) = text.strip_prefix(EDIT_PREFIX) else { continue };
        let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
        if let Ok(bytes) = B64U.decode(b64.trim()) {
            if let Ok(p) = serde_json::from_slice::<Value>(&bytes) {
                if let (Some(id), Some(new_text)) = (
                    p.get("id").and_then(Value::as_str),
                    p.get("text").and_then(Value::as_str),
                ) {
                    map.insert((id.to_string(), mine), new_text.to_string());
                }
            }
        }
    }
    map
}

/// Apply any edit to a conversation row: swap the text + mark `edited`.
fn apply_edit(m: Value, edits: &std::collections::HashMap<(String, bool), String>) -> Value {
    let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
    let id = m.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    if let Some(new_text) = edits.get(&(id, mine)) {
        let mut m2 = m;
        if let Some(obj) = m2.as_object_mut() {
            obj.insert("text".into(), json!(new_text));
            obj.insert("edited".into(), json!(true));
        }
        m2
    } else {
        m
    }
}

/// Edit one of MY messages for everyone. No-op on empty ids/text.
pub async fn edit_chat_message(chat_id: &str, msg_id: &str, new_text: &str, is_group: bool) -> bool {
    if chat_id.is_empty() || msg_id.is_empty() || new_text.trim().is_empty() {
        return false;
    }
    let b64 = B64U.encode(json!({ "id": msg_id, "text": new_text }).to_string().as_bytes());
    let payload = format!("{EDIT_PREFIX}{b64}");
    if is_group {
        dms::send_group_message(chat_id, &payload).await.is_ok()
    } else {
        chat_send(chat_id, &payload).await.is_ok()
    }
}

fn call_seen() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Send a call-control signal to a contact (E2E, hidden). `payload` is the caller-built JSON
/// (e.g. {"type":"offer","call_id":"…"}). Rides the normal DM transport.
pub async fn call_send(to_did: &str, payload: &str) -> bool {
    if to_did.is_empty() {
        return false;
    }
    let b64 = B64U.encode(payload.as_bytes());
    chat_send(to_did, &format!("{CALL_PREFIX}{b64}")).await.is_ok()
}

/// Hey Verse lane: sealed + ratcheted like a DM on the wire, but the receiver
/// diverts it into an in-memory inbox — it never lands in the conversation,
/// never counts as unread, never notifies, and never competes with call
/// signaling. Use for world presence: invites, movement, in-world chat.
pub async fn verse_send(to_did: &str, payload: &str) -> bool {
    if to_did.is_empty() {
        return false;
    }
    let b64 = B64U.encode(payload.as_bytes());
    chat_send(to_did, &format!("{}{}", hey_core::api::dms::VERSE_PREFIX, b64))
        .await
        .is_ok()
}

/// Drain the verse inbox → `[{ "from": <did>, "payload": <json> }]`.
pub fn verse_poll() -> Value {
    let mut out: Vec<Value> = Vec::new();
    for (from, b64) in hey_core::api::dms::verse_drain() {
        if let Ok(bytes) = B64U.decode(b64.trim()) {
            if let Ok(payload) = serde_json::from_slice::<Value>(&bytes) {
                out.push(serde_json::json!({ "from": from, "payload": payload }));
            }
        }
    }
    Value::Array(out)
}

/// Poll for inbound call signals across all DM contacts. Returns NEW signals (each emitted once)
/// as `[{ "from": <did>, "payload": <json> }]`, ignoring stale (>2 min) ones so only live rings
/// surface. The native CallManager polls this ~1s while the app is open.
pub async fn call_poll() -> Value {
    ensure_session().await.ok();
    let now = hey_core::plat::now_ms();
    // Phase 1 (async): gather candidate inbound call signals. We must NOT hold the std Mutex guard
    // across an .await (it isn't Send), so dedup happens in phase 2.
    let mut candidates: Vec<(String, String, Value)> = Vec::new(); // (msg id, from did, payload)
    for c in dms::list_contacts().await {
        let conv = dms::read_conversation(&c.did).await;
        let arr = serde_json::to_value(&conv)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        for m in arr.iter().rev().take(8) {
            if m.get("mine").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
            if now - ts > 120_000 {
                continue; // stale — only ring on live signals
            }
            let text = m.get("text").and_then(Value::as_str).unwrap_or("");
            let Some(b64) = text.strip_prefix(CALL_PREFIX) else {
                continue;
            };
            let id = m
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}:{}", c.did, ts));
            if let Ok(bytes) = B64U.decode(b64.trim()) {
                if let Ok(payload) = serde_json::from_slice::<Value>(&bytes) {
                    candidates.push((id, c.did.clone(), payload));
                }
            }
        }
    }
    // Phase 2 (sync): dedup against the seen-set + emit each signal once.
    let mut out: Vec<Value> = Vec::new();
    if let Ok(mut seen) = call_seen().lock() {
        for (id, from, payload) in candidates {
            if seen.insert(id) {
                out.push(json!({ "from": from, "payload": payload }));
            }
        }
        if seen.len() > 1000 {
            seen.clear();
        }
    }
    json!(out)
}

// ── group voice calls (mesh) ──────────────────────────────────────────────────
//
// A group call is announced as a control message on the group thread (so it reaches every member
// over the same E2E gossip as chat). Members tap "Join" to enter the audio mesh. Each control
// message carries the sender's voice ticket (carrier EndpointAddr) so peers can dial each other
// directly — no server, no mixer. The native CallManager polls `group_call_roster` to drive the
// mesh roster (who to dial) and the participant UI. These messages are hidden from the chat thread.
const GCALL_PREFIX: &str = "\u{1}hey-gcall:1:";

/// (my did, my voice ticket, my display name) for embedding in a group-call control message.
async fn my_call_identity() -> (String, String, String) {
    let did = whoami_did().await.unwrap_or_default();
    let ticket = runtime::peer::my_ticket().await.unwrap_or_default();
    let name = my_profile()
        .await
        .get("nickname")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    (did, ticket, name)
}

async fn post_gcall(gid: &str, payload: &Value) -> bool {
    let b64 = B64U.encode(payload.to_string().as_bytes());
    dms::send_group_message(gid, &format!("{GCALL_PREFIX}{b64}"))
        .await
        .is_ok()
}

/// Start a group call: announce it on the group thread + return the new `call_id` and my ticket.
/// The native layer then opens the audio mesh and begins polling the roster.
pub async fn group_call_start(gid: &str) -> Value {
    ensure_session().await.ok();
    let (did, ticket, name) = my_call_identity().await;
    if did.is_empty() {
        return json!({ "ok": false });
    }
    let tail: String = did.chars().rev().take(6).collect();
    let call_id = format!("gc-{}-{}", tail, hey_core::plat::now_ms());
    let payload = json!({ "t": "start", "call_id": call_id, "did": did, "ticket": ticket, "name": name });
    let ok = post_gcall(gid, &payload).await;
    json!({ "ok": ok, "call_id": call_id, "ticket": ticket })
}

/// Emit a group-call control signal: `join` (entering / heartbeat), `leave` (I left), or `end`
/// (host ended for everyone).
pub async fn group_call_signal(gid: &str, call_id: &str, kind: &str) -> bool {
    if call_id.is_empty() {
        return false;
    }
    let (did, ticket, name) = my_call_identity().await;
    let payload = json!({ "t": kind, "call_id": call_id, "did": did, "ticket": ticket, "name": name });
    post_gcall(gid, &payload).await
}

/// Derive the live state of a group call from the group thread: who's present (latest start/join and
/// not since left), each participant's dialable ticket, whether the host ended it, and whether it's
/// still active (recent + non-empty). Drives both the mesh roster and the in-call participant list.
pub async fn group_call_roster(gid: &str, call_id: &str) -> Value {
    ensure_session().await.ok();
    let me = whoami_did().await.unwrap_or_default();
    let now = hey_core::plat::now_ms();
    let conv = dms::read_group_conversation(gid).await;
    let arr = serde_json::to_value(&conv)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    // did -> (latest ts, present, ticket, name)
    let mut state: std::collections::HashMap<String, (i64, bool, String, String)> = std::collections::HashMap::new();
    let mut ended = false;
    let mut latest_ts = 0i64;
    for m in arr.iter() {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let Some(b64) = text.strip_prefix(GCALL_PREFIX) else {
            continue;
        };
        let Ok(bytes) = B64U.decode(b64.trim()) else { continue };
        let Ok(p) = serde_json::from_slice::<Value>(&bytes) else { continue };
        if p.get("call_id").and_then(Value::as_str) != Some(call_id) {
            continue;
        }
        let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
        let kind = p.get("t").and_then(Value::as_str).unwrap_or("");
        let did = p.get("did").and_then(Value::as_str).unwrap_or("").to_string();
        let ticket = p.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
        let name = p.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        latest_ts = latest_ts.max(ts);
        if kind == "end" {
            ended = true;
        }
        if did.is_empty() {
            continue;
        }
        let e = state.entry(did).or_insert((0, false, String::new(), String::new()));
        if ts >= e.0 {
            e.0 = ts;
            match kind {
                "start" | "join" => {
                    e.1 = true;
                    if !ticket.is_empty() {
                        e.2 = ticket;
                    }
                    if !name.is_empty() {
                        e.3 = name;
                    }
                }
                "leave" => e.1 = false,
                _ => {}
            }
        }
    }
    let participants: Vec<Value> = state
        .iter()
        .filter(|(_, v)| v.1)
        .map(|(did, v)| json!({ "did": did, "ticket": v.2, "name": v.3, "mine": *did == me }))
        .collect();
    let stale = now - latest_ts > 120_000;
    let active = !ended && !stale && !participants.is_empty();
    json!({ "active": active, "ended": ended, "call_id": call_id, "participants": participants })
}

/// Find the most recently announced group call on this thread (the latest "start") and return its
/// live roster — so the UI can offer a "Join" without already knowing the call_id. `active:false`
/// when there's no call. The returned object carries `call_id` for the matched call.
pub async fn group_active_call(gid: &str) -> Value {
    ensure_session().await.ok();
    let conv = dms::read_group_conversation(gid).await;
    let arr = serde_json::to_value(&conv)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut latest: Option<(i64, String)> = None;
    for m in arr.iter() {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let Some(b64) = text.strip_prefix(GCALL_PREFIX) else { continue };
        let Ok(bytes) = B64U.decode(b64.trim()) else { continue };
        let Ok(p) = serde_json::from_slice::<Value>(&bytes) else { continue };
        if p.get("t").and_then(Value::as_str) != Some("start") {
            continue;
        }
        let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
        let cid = p.get("call_id").and_then(Value::as_str).unwrap_or("").to_string();
        if cid.is_empty() {
            continue;
        }
        if latest.as_ref().map_or(true, |(t, _)| ts > *t) {
            latest = Some((ts, cid));
        }
    }
    match latest {
        Some((_, cid)) => group_call_roster(gid, &cid).await,
        None => json!({ "active": false, "ended": false, "participants": [] }),
    }
}

/// Send my published tip addresses to a contact over the E2E DM channel, ONCE per
/// peer. Idempotent on the receiver (cache merge). Returns false if I have no
/// addresses yet (provision first) or there's no DM channel to them.
pub async fn share_addresses(to_did: &str) -> bool {
    if to_did.is_empty() {
        return false;
    }
    let addrs = my_profile().await.get("addresses").cloned().unwrap_or(Value::Null);
    if !addrs.is_object() {
        return false; // wallet not provisioned/published yet
    }
    {
        let _g = storage_lock().lock().await;
        let sent: Vec<Value> = shared_read_json("hey-social/addr-shared.json")
            .await.ok().flatten().and_then(|v| v.as_array().cloned()).unwrap_or_default();
        if sent.iter().any(|d| d.as_str() == Some(to_did)) {
            return true; // already shared
        }
    }
    let payload = B64U.encode(addrs.to_string().as_bytes());
    let ok = chat_send(to_did, &format!("{ADDR_PREFIX}{payload}")).await.is_ok();
    if ok {
        let _g = storage_lock().lock().await;
        let mut sent: Vec<Value> = shared_read_json("hey-social/addr-shared.json")
            .await.ok().flatten().and_then(|v| v.as_array().cloned()).unwrap_or_default();
        if !sent.iter().any(|d| d.as_str() == Some(to_did)) {
            sent.push(json!(to_did));
            let _ = shared_write_json("hey-social/addr-shared.json", &json!(sent)).await;
        }
    }
    ok
}

/// Cache a peer's DM-shared addresses into their profile cache so resolve_tip finds
/// them — the spoof surface is the same as their other DMs (E2E, authenticated peer).
async fn cache_peer_addresses(did: &str, addrs: Value) {
    if !addrs.is_object() {
        return;
    }
    let _g = storage_lock().lock().await;
    let key = format!("hey-social/peers/{did}.json");
    let mut p = shared_read_json(&key).await.ok().flatten().unwrap_or_else(|| json!({}));
    if !p.is_object() {
        p = json!({});
    }
    p["addresses"] = addrs;
    let _ = shared_write_json(&key, &p).await;
}

/// Tip-sheet entry point: make sure a contact has my addresses + cache theirs (via
/// the DM channel), then return their resolved tip addresses. Makes tipping a chat
/// contact "just work" without a follow. For a non-contact this degrades to the
/// plain cached/feed lookup (resolve_tip).
pub async fn refresh_contact_addresses(did: &str) -> Value {
    let _ = chat_conversation(did).await; // shares mine + caches any card they sent
    resolve_tip(did).await
}

/// Raw stored profile for `did` (NO short-DID fallback) — used to overlay the LIVE nickname/avatar
/// onto posts so a profile edit shows on old posts too. Empty object if we hold no cached profile.
async fn raw_profile(did: &str, me: &str) -> Value {
    let file = if did.is_empty() || did == me {
        "hey-social/profile.json".to_string()
    } else {
        format!("hey-social/peers/{did}.json")
    };
    shared_read_json(&file).await.ok().flatten().unwrap_or_else(|| json!({}))
}

/// Overlay the author's CURRENT cached nickname/avatar onto a post in place (so an identity change
/// shows on OLD posts, not just new ones). The post's stored author_name/avatar stay as the fallback
/// when we have no cached profile. `cache` memoizes per-author lookups across a feed build.
async fn overlay_author(p: &mut Value, me: &str, cache: &mut std::collections::HashMap<String, Value>) {
    let Some(author) = p.get("author").and_then(Value::as_str).map(str::to_string) else {
        return;
    };
    if !cache.contains_key(&author) {
        let prof = raw_profile(&author, me).await;
        cache.insert(author.clone(), prof);
    }
    let prof = &cache[&author];
    if let Some(n) = prof.get("nickname").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        p["author_name"] = json!(n);
    }
    if let Some(a) = prof.get("avatar").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        p["author_avatar"] = json!(a);
    }
}

/// Profile for `did` ("" = me): from my profile or the cached peer profile,
/// falling back to a short-DID nickname.
pub async fn get_profile(did: &str) -> Value {
    let me = whoami_did().await.unwrap_or_default();
    let target = if did.is_empty() { me.clone() } else { did.to_string() };
    let file = if did.is_empty() || did == me {
        "hey-social/profile.json".to_string()
    } else {
        format!("hey-social/peers/{did}.json")
    };
    let p = shared_read_json(&file).await.ok().flatten().unwrap_or_else(|| json!({}));
    let short = target.trim_start_matches("did:key:z").chars().take(10).collect::<String>();
    json!({
        "did": target,
        "nickname": p.get("nickname").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(&short),
        "bio": p.get("bio").and_then(Value::as_str).unwrap_or(""),
        "avatar": p.get("avatar").and_then(Value::as_str).unwrap_or(""),
        "addresses": p.get("addresses").cloned().unwrap_or(Value::Null),
    })
}

/// Delete my post locally + tell followers to remove it (signed delete event).
pub async fn delete_post(id: &str) -> Result<Value, String> {
    let me = whoami_did().await?;
    let post = shared_read_json(&format!("hey-social/posts/{id}.json")).await.ok().flatten();
    if post.as_ref().and_then(|p| p.get("author")).and_then(Value::as_str) != Some(me.as_str()) {
        return err("not your post");
    }
    remove_post_local(id).await;
    publish(&feed_topic(&me), "hey-social.post_delete", json!({ "id": id })).await;
    Ok(json!({ "ok": true }))
}

/// Edit my post's caption + re-broadcast (followers overwrite by id).
pub async fn edit_post(id: &str, caption: &str) -> Result<Value, String> {
    let me = whoami_did().await?;
    let mut post = shared_read_json(&format!("hey-social/posts/{id}.json"))
        .await
        .ok()
        .flatten()
        .ok_or("post not found")?;
    if post.get("author").and_then(Value::as_str) != Some(me.as_str()) {
        return err("not your post");
    }
    post["caption"] = json!(caption);
    shared_write_json(&format!("hey-social/posts/{id}.json"), &post).await.map_err(|e| e.to_string())?;
    publish(&feed_topic(&me), "hey-social.post", post.clone()).await;
    Ok(post)
}

async fn remove_post_local(id: &str) {
    let _g = storage_lock().lock().await;
    // Drop from the feed index (feed() only reads indexed ids) + tombstone the file.
    let mut idx: Vec<Value> = shared_read_json(FEED_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    idx.retain(|v| v.as_str() != Some(id));
    let _ = shared_write_json(FEED_INDEX, &json!(idx)).await;
    let _ = shared_write_json(&format!("hey-social/posts/{id}.json"), &Value::Null).await;
}

/// Most-recent-first list of posts (own feed, v1).
pub async fn feed(limit: usize) -> Result<Value, String> {
    let me = whoami_did().await.unwrap_or_default();
    let idx: Vec<Value> = shared_read_json(FEED_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut cache: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for id in idx.iter().filter_map(Value::as_str) {
        if let Ok(Some(mut p)) = shared_read_json(&format!("hey-social/posts/{id}.json")).await {
            if p.is_object() {
                overlay_author(&mut p, &me, &mut cache).await; // live name/avatar onto old posts
                out.push(p);
            }
        }
    }
    // Newest first — my posts + ingested remote posts interleave by timestamp.
    out.sort_by(|a, b| {
        b.get("ts").and_then(Value::as_i64).unwrap_or(0)
            .cmp(&a.get("ts").and_then(Value::as_i64).unwrap_or(0))
    });
    out.truncate(limit);
    Ok(json!(out))
}

pub async fn get_post(id: &str) -> Result<Value, String> {
    match shared_read_json(&format!("hey-social/posts/{id}.json")).await {
        Ok(Some(mut p)) => {
            let me = whoami_did().await.unwrap_or_default();
            let mut cache = std::collections::HashMap::new();
            overlay_author(&mut p, &me, &mut cache).await;
            Ok(p)
        }
        Ok(None) => err("post not found"),
        Err(e) => err(format!("get_post: {e}")),
    }
}

fn new_id() -> String {
    // Random 16-byte id, hex. On the (rare) getrandom failure, fall back to
    // time+counter so we NEVER return a constant all-zero id (which would make
    // every post/comment collide on one storage key + dedup key).
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        let t = hey_core::plat::now_ms() as u64;
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        return format!("{t:016x}{c:016x}");
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ── reactions ────────────────────────────────────────────────────────────────

fn reactions_summary(map: &serde_json::Map<String, Value>, me: &str) -> Value {
    let mut counts: std::collections::BTreeMap<String, i64> = Default::default();
    let mut mine = Value::Null;
    for (did, e) in map {
        if let Some(es) = e.as_str() {
            *counts.entry(es.to_string()).or_insert(0) += 1;
            if did == me {
                mine = json!(es);
            }
        }
    }
    json!({ "counts": counts, "mine": mine, "total": map.len() })
}

/// Toggle the current user's reaction on a post. Returns the updated summary.
pub async fn react(post_id: &str, emoji: &str) -> Result<Value, String> {
    let me = whoami_did().await?;
    let key = format!("hey-social/reactions/{post_id}.json");
    let (map, already) = {
        let _g = storage_lock().lock().await;
        let mut map = shared_read_json(&key)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let already = map.get(&me).and_then(Value::as_str) == Some(emoji);
        if already {
            map.remove(&me);
        } else {
            map.insert(me.clone(), json!(emoji));
        }
        shared_write_json(&key, &Value::Object(map.clone()))
            .await
            .map_err(|e| e.to_string())?;
        (map, already)
    };
    // Broadcast the reaction on the POST AUTHOR's topic so they + their other
    // followers see it. (Signed by us; the op = set/unset.)
    if let Ok(p) = get_post(post_id).await {
        if let Some(author) = p.get("author").and_then(Value::as_str) {
            let op = if already { "unset" } else { "set" };
            publish(
                &feed_topic(author),
                "hey-social.react",
                json!({ "post_id": post_id, "emoji": emoji, "op": op }),
            )
            .await;
        }
    }
    Ok(reactions_summary(&map, &me))
}

pub async fn get_reactions(post_id: &str) -> Result<Value, String> {
    let me = whoami_did().await.unwrap_or_default();
    let key = format!("hey-social/reactions/{post_id}.json");
    let map = shared_read_json(&key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    Ok(reactions_summary(&map, &me))
}

// ── comments ─────────────────────────────────────────────────────────────────

pub async fn add_comment(post_id: &str, text: &str, parent_id: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return err("empty comment");
    }
    let me = whoami_did().await?;
    let prof = my_profile().await;
    let key = format!("hey-social/comments/{post_id}.json");
    let c = json!({
        "id": new_id(), "author": me,
        "author_name": prof.get("nickname").and_then(Value::as_str).unwrap_or(""),
        "text": text, "ts": hey_core::plat::now_ms(),
        "parent": parent_id,
    });
    {
        let _g = storage_lock().lock().await;
        let mut list = shared_read_json(&key)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        list.push(c.clone());
        shared_write_json(&key, &json!(list)).await.map_err(|e| e.to_string())?;
    }
    // Broadcast on the post author's topic.
    if let Ok(p) = get_post(post_id).await {
        if let Some(author) = p.get("author").and_then(Value::as_str) {
            publish(
                &feed_topic(author),
                "hey-social.comment",
                json!({ "post_id": post_id, "comment": c }),
            )
            .await;
        }
    }
    Ok(c)
}

pub async fn get_comments(post_id: &str) -> Result<Value, String> {
    let key = format!("hey-social/comments/{post_id}.json");
    Ok(shared_read_json(&key).await.ok().flatten().unwrap_or_else(|| json!([])))
}

// ── follow / social graph ────────────────────────────────────────────────────

/// A shareable follow link: `hey:follow:<base64url(json{did,ticket,x,k})>`.
/// Carries our DM pubkeys (x=X25519, k=ML-KEM) so following also makes us
/// chat-able — the other side can "Message" us with no extra handshake.
pub async fn my_friend_link() -> Result<String, String> {
    let w = whoami().await?;
    let pk = dms::my_pubkeys().await;
    // Trim direct-IP addrs from the ticket so the link/QR stays small + scannable;
    // the peer connects via the relay, then iroh upgrades to a direct path anyway.
    let ticket = compact_ticket(w.get("ticket").and_then(Value::as_str).unwrap_or(""));
    let payload = json!({
        "did": w.get("did"),
        "ticket": ticket,
        "x": pk.as_ref().map(|k| k.x25519_pub_b64.clone()),
        "k": pk.as_ref().map(|k| k.ml_kem_pub_b64.clone()),
    });
    Ok(format!("hey:follow:{}", B64U.encode(payload.to_string())))
}

/// Slim a carrier ticket for the shareable friend link/QR: keep the node id +
/// relay + a SMALL set of direct IP addrs. Round-trips with the same base32 the
/// carrier uses (legacy base64 tolerated); falls back to the input on any error.
///
/// We deliberately RETAIN direct IPs (capped) rather than stripping them to
/// relay-only: when two devices are on the SAME LAN, the relay may be slow or
/// unreachable and pkarr/DNS resolution is dead on many networks, so a scan that
/// carries the peer's LAN address lets the carrier inject a directly-dialable
/// candidate into its in-RAM address store and mesh WITHOUT any relay/hole-punch.
/// The cap keeps the QR scannable; a peer that can't reach a given IP just ignores
/// it, so including extras never regresses the relay path.
fn compact_ticket(ticket: &str) -> String {
    const MAX_IP_ADDRS: usize = 4;
    // Carrier tickets are base32 (upstream-aligned); tolerate legacy base64 too.
    let bytes = data_encoding::BASE32_NOPAD
        .decode(ticket.as_bytes())
        .ok()
        .or_else(|| B64U.decode(ticket).ok());
    let Some(bytes) = bytes else { return ticket.to_string() };
    let Ok(mut v) = serde_json::from_slice::<Value>(&bytes) else { return ticket.to_string() };
    if let Some(addrs) = v.get_mut("addrs").and_then(|a| a.as_array_mut()) {
        // TransportAddr is an externally-tagged enum: {"Relay":..} | {"Ip":..}.
        // Keep ALL relays; keep up to MAX_IP_ADDRS direct IPs (the same-LAN
        // direct-dial candidates), dropping the rest to bound the QR size.
        let mut ip_kept = 0usize;
        addrs.retain(|e| {
            if e.get("Relay").is_some() {
                true
            } else if e.get("Ip").is_some() && ip_kept < MAX_IP_ADDRS {
                ip_kept += 1;
                true
            } else {
                false
            }
        });
    }
    match serde_json::to_vec(&v) {
        Ok(b) => data_encoding::BASE32_NOPAD.encode(&b),
        Err(_) => ticket.to_string(),
    }
}

/// Accept either a raw `did:key:z…` or a `hey:follow:…` link.
/// Returns (did, ticket, Option<(x25519_pub_b64, ml_kem_pub_b64)>).
fn parse_follow(input: &str) -> Option<(String, String, Option<(String, String)>)> {
    let s = input.trim();
    if let Some(rest) = s.strip_prefix("hey:follow:") {
        if let Ok(bytes) = B64U.decode(rest) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                let did = v.get("did").and_then(Value::as_str)?.to_string();
                let ticket = v.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
                let keys = match (v.get("x").and_then(Value::as_str), v.get("k").and_then(Value::as_str)) {
                    (Some(x), Some(k)) => Some((x.to_string(), k.to_string())),
                    _ => None,
                };
                return Some((did, ticket, keys));
            }
        }
    }
    if s.starts_with("did:key:z") {
        return Some((s.to_string(), String::new(), None));
    }
    None
}

/// Bootstrap a DM-capable contact from a peer's advertised pubkeys (+ticket).
async fn bootstrap_dm(did: &str, keys: &Option<(String, String)>, ticket: &str) {
    if let Some((x, k)) = keys {
        let _ = dms::bootstrap_contact_from_keys(
            did,
            "",
            dms::PeerKeys { x25519_pub_b64: x.clone(), ml_kem_pub_b64: k.clone() },
            if ticket.is_empty() { None } else { Some(ticket.to_string()) },
            false,
        )
        .await;
    }
}

pub async fn follow(input: &str) -> Result<Value, String> {
    let (did, ticket, keys) = parse_follow(input).ok_or("not a valid DID or friend link")?;
    let me = whoami_did().await.unwrap_or_default();
    if did == me {
        return err("that's your own DID");
    }
    {
        let _g = storage_lock().lock().await;
        let mut list = shared_read_json("hey-social/following.json")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if !list.iter().any(|e| e.get("did").and_then(Value::as_str) == Some(did.as_str())) {
            list.push(json!({
                "did": did, "ticket": ticket, "ts": hey_core::plat::now_ms(),
                "x": keys.as_ref().map(|(x, _)| x.clone()),
                "k": keys.as_ref().map(|(_, k)| k.clone()),
            }));
            shared_write_json("hey-social/following.json", &json!(list)).await.ok();
        }
    }
    bootstrap_dm(&did, &keys, &ticket).await;
    // Join their feed topic + backfill request.
    let topic = feed_topic(&did);
    if ticket.is_empty() {
        let _ = runtime::peer::join_topic(&topic).await;
    } else {
        let _ = runtime::peer::connect(&ticket).await;
        let _ = runtime::peer::join_topic_with(&topic, &[ticket.clone()]).await;
    }
    publish(&topic, "hey-social.sync_req", json!({ "want": "backfill" })).await;
    // Announce ourselves (with OUR keys) so they get us as a follower + can DM back.
    let myk = dms::my_pubkeys().await;
    let myt = runtime::peer::my_ticket().await.unwrap_or_default();
    publish(
        &topic,
        "hey-social.follow",
        json!({ "ticket": myt, "x": myk.as_ref().map(|k| k.x25519_pub_b64.clone()), "k": myk.as_ref().map(|k| k.ml_kem_pub_b64.clone()) }),
    )
    .await;
    Ok(json!({ "ok": true, "did": did }))
}

pub async fn followers() -> Result<Value, String> {
    Ok(shared_read_json("hey-social/followers.json").await.ok().flatten().unwrap_or_else(|| json!([])))
}

/// Follow back a known follower — reuses their recorded ticket+keys so the
/// follow is DM-capable, then runs the normal follow flow.
pub async fn follow_back(did: &str) -> Result<Value, String> {
    let list = shared_read_json("hey-social/followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let r = list
        .iter()
        .find(|e| e.get("did").and_then(Value::as_str) == Some(did))
        .ok_or("not a follower")?;
    let link = format!(
        "hey:follow:{}",
        B64U.encode(
            json!({ "did": did, "ticket": r.get("ticket"), "x": r.get("x"), "k": r.get("k") }).to_string()
        )
    );
    follow(&link).await
}

/// Ensure a DM contact exists for `did` (bootstrapping from a known follow
/// record's keys if needed), so the UI can open a conversation.
pub async fn start_chat(did: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    if dms::find_contact(did).await.is_some() {
        return Ok(json!({ "ok": true, "did": did }));
    }
    for file in ["hey-social/following.json", "hey-social/followers.json"] {
        let list = shared_read_json(file).await.ok().flatten().and_then(|v| v.as_array().cloned()).unwrap_or_default();
        if let Some(r) = list.iter().find(|e| e.get("did").and_then(Value::as_str) == Some(did)) {
            if let (Some(x), Some(k)) = (r.get("x").and_then(Value::as_str), r.get("k").and_then(Value::as_str)) {
                let ticket = r.get("ticket").and_then(Value::as_str).unwrap_or("");
                bootstrap_dm(did, &Some((x.to_string(), k.to_string())), ticket).await;
                return Ok(json!({ "ok": true, "did": did }));
            }
        }
    }
    err("no chat keys for this user yet (they need to share a Hey link)")
}

/// Posts by one author that we've ingested (their grid on a profile view).
pub async fn user_posts(did: &str) -> Value {
    let f = feed(300).await.unwrap_or_else(|_| json!([]));
    let arr: Vec<Value> = f
        .as_array()
        .map(|a| a.iter().filter(|p| p.get("author").and_then(Value::as_str) == Some(did)).cloned().collect())
        .unwrap_or_default();
    json!(arr)
}

pub async fn delete_conversation(did: &str) -> Result<Value, String> {
    dms::delete_conversation(did).await.map(|_| json!({ "ok": true }))
}
pub async fn delete_group(gid: &str) -> Result<Value, String> {
    dms::delete_group(gid).await.map(|_| json!({ "ok": true }))
}

pub async fn unfollow(did: &str) -> Result<Value, String> {
    let mut list = shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    list.retain(|e| e.get("did").and_then(Value::as_str) != Some(did));
    shared_write_json("hey-social/following.json", &json!(list))
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true }))
}

pub async fn following() -> Result<Value, String> {
    Ok(shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| json!([])))
}

pub async fn is_following(did: &str) -> Result<Value, String> {
    let list = shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Ok(json!({
        "following": list.iter().any(|e| e.get("did").and_then(Value::as_str) == Some(did))
    }))
}

// ── cross-device receiver (the carrier feed) ─────────────────────────────────

async fn followed_dids() -> Vec<String> {
    following()
        .await
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|e| e.get("did").and_then(Value::as_str).map(String::from))
        .collect()
}

/// Join my own topic (to receive reactions/comments/sync_req on my posts) plus
/// every followed author's topic. Idempotent.
pub async fn ensure_subscriptions() {
    let me = match whoami_did().await {
        Ok(d) => d,
        Err(_) => return,
    };
    let _ = runtime::peer::join_topic(&feed_topic(&me)).await;
    let list = following().await.ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    for e in &list {
        if let Some(did) = e.get("did").and_then(Value::as_str) {
            let ticket = e.get("ticket").and_then(Value::as_str).unwrap_or("");
            if ticket.is_empty() {
                let _ = runtime::peer::join_topic(&feed_topic(did)).await;
            } else {
                let _ = runtime::peer::connect(ticket).await;
                let _ = runtime::peer::join_topic_with(&feed_topic(did), &[ticket.to_string()]).await;
            }
        }
    }
    // Self-heal: rebuild DM contacts from the follow stores so EXISTING
    // relationships are DM-capable after a restart/upgrade WITHOUT re-pairing.
    // (Previously a re-pair was needed because the live follow handler only
    // bootstrapped a contact for a NEWLY-recorded follower — so a followee who
    // already had the peer in followers.json never subscribed to the per-pair
    // queue the peer was already sending DMs on.)
    reconcile_dm_contacts().await;
}

/// Bootstrap a DM contact for every follower / followed peer whose stored record
/// carries PQ keys. Idempotent (bootstrap_contact_from_keys no-ops on an
/// unchanged pin), so it is safe to run on every boot. This is what lets a
/// followee subscribe to the deterministic per-pair queue the follower already
/// sends on — the missing half of two-way DM bootstrap.
async fn reconcile_dm_contacts() {
    for file in ["hey-social/followers.json", "hey-social/following.json"] {
        let list = shared_read_json(file)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        for e in &list {
            let did = match e.get("did").and_then(Value::as_str) {
                Some(d) if !d.is_empty() => d,
                _ => continue,
            };
            let ticket = e.get("ticket").and_then(Value::as_str).unwrap_or("");
            if let (Some(x), Some(k)) =
                (e.get("x").and_then(Value::as_str), e.get("k").and_then(Value::as_str))
            {
                bootstrap_dm(did, &Some((x.to_string(), k.to_string())), ticket).await;
            }
        }
    }
}

/// Drain new gossip messages on all subscribed topics and ingest them. Returns
/// how many new items were stored. Called on a ~2s loop by the receiver thread.
pub async fn poll_once() -> usize {
    let me = match whoami_did().await {
        Ok(d) => d,
        Err(_) => return 0,
    };
    // Loud-and-closed: re-drive any social broadcast the carrier couldn't take
    // earlier (see publish/queue_broadcast) before ingesting new traffic.
    flush_pending_broadcasts().await;
    // Evict half-received chunk sets that stalled (frees memory; the next
    // ensure_blob re-requests after the retry window).
    {
        let now = hey_core::plat::now_ms();
        crate::lock_safe(pending_blobs()).retain(|_, (t, _)| now - *t < PENDING_TTL_MS);
    }
    let mut topics = vec![feed_topic(&me)];
    for did in followed_dids().await {
        topics.push(feed_topic(&did));
    }
    let mut count = 0usize;
    for topic in topics {
        let resp = runtime::peer::recv(runtime::peer::RecvArgs {
            topic: &topic,
            limit: 128,
            consumer_id: RECV_CONSUMER,
            // Skip our own echoes — local state already applied synchronously.
            skip_sender_id: Some(&me),
        })
        .await;
        if let Ok(r) = resp {
            let msgs = r
                .get("data")
                .and_then(|d| d.get("messages"))
                .or_else(|| r.get("messages"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for m in msgs {
                if let Some(content) = m
                    .get("content")
                    .or_else(|| m.get("message"))
                    .and_then(Value::as_str)
                {
                    if ingest(&topic, content, &me).await {
                        count += 1;
                    }
                }
            }
        }
    }
    if count > 0 {
        bump_rev(); // signal the UI to auto-refresh
    }
    count
}

/// True if an incoming post names ME — an `@nickname` in the caption with a
/// right-hand word boundary (so `@bob` doesn't fire for `@bobby`), or an explicit
/// `mentions` array carrying my did BUT only when the caption actually has an
/// `@`-token (the array is sender-asserted, so a no-@ post can never ring as a
/// mention). Drives the "mentioned you" notification vs a plain new-post one.
async fn mentions_me(payload: &Value, me: &str) -> bool {
    let caption = payload.get("caption").and_then(Value::as_str).unwrap_or("");
    if caption.is_empty() {
        return false;
    }
    let lc = caption.to_lowercase();
    let nick = my_profile().await.get("nickname").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    if nick.len() >= 2 {
        let needle = format!("@{nick}");
        // match_indices yields byte offsets; needle is ASCII-prefixed so the slice
        // after it is a valid char boundary.
        for (idx, _) in lc.match_indices(&needle) {
            let after = &lc[idx + needle.len()..];
            let boundary = match after.chars().next() {
                None => true,                                  // end of caption
                Some(c) => !(c.is_alphanumeric() || c == '_'), // not a handle char
            };
            if boundary {
                return true;
            }
        }
    }
    if lc.contains('@') {
        if let Some(arr) = payload.get("mentions").and_then(|v| v.as_array()) {
            if arr.iter().any(|m| m.as_str() == Some(me)) {
                return true;
            }
        }
    }
    false
}

/// Verify + apply one received event. `topic` is where it arrived (used to bind
/// posts to their author's topic and to answer sync only on our own topic).
async fn ingest(topic: &str, wire: &str, me: &str) -> bool {
    let ev = match events::from_wire_string(wire) {
        Some(e) => e,
        None => return false,
    };
    if events::verify_signed_event(&ev) != VerifyResult::Valid {
        return false;
    }
    match ev.event_type.as_str() {
        "hey-social.post" => {
            // A post is only valid on ITS author's topic, and author == signer.
            let author = ev.payload.get("author").and_then(Value::as_str).unwrap_or("");
            if author != ev.sender_did || topic != feed_topic(author) {
                return false;
            }
            let id = match ev.payload.get("id").and_then(Value::as_str) {
                Some(i) => i.to_string(),
                None => return false,
            };
            let key = format!("hey-social/posts/{id}.json");
            let existed = shared_read_json(&key).await.ok().flatten().is_some();
            let _ = shared_write_json(&key, &ev.payload).await;
            add_to_index(&id).await;
            // Pull the post's media bytes + the author's avatar over the carrier.
            if let Some(arr) = ev.payload.get("media").and_then(|v| v.as_array()) {
                for m in arr {
                    if let Some(cid) = m.get("cid").and_then(Value::as_str) {
                        ensure_blob(cid, author).await;
                    }
                }
            }
            if let Some(cid) = ev.payload.get("author_avatar").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                ensure_blob(cid, author).await;
            }
            if !existed {
                let name = ev.payload.get("author_name").and_then(Value::as_str).filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| author.trim_start_matches("did:key:z").chars().take(10).collect());
                // A post that names you gets a louder "mentioned you" notification.
                // key = post id so two distinct posts by one author don't collapse.
                if mentions_me(&ev.payload, me).await {
                    push_notif("mention", &name, "mentioned you in a post", author, &id);
                } else {
                    push_notif("post", &name, "shared a new post", author, &id);
                }
            }
            !existed
        }
        "hey-social.blob_req" => {
            // Only the topic OWNER (post author) answers, on their own topic —
            // prevents every holder re-broadcasting the media to the whole topic.
            if topic == feed_topic(me) {
                if let Some(cid) = ev.payload.get("cid").and_then(Value::as_str) {
                    if let Ok(bytes) = content::get_bytes(cid, None).await {
                        send_blob(cid, &bytes, topic).await;
                    }
                }
            }
            false
        }
        "hey-social.blob" => {
            // `n`/`i`/`b64` are attacker-controlled. Bound them so one forged event
            // cannot force an unbounded `vec![None; n]` pre-allocation (remote OOM):
            // before this guard, `n = 4_000_000_000` allocated ~96 GB and aborted.
            // Legit transfers chunk at BLOB_CHUNK bytes and large media rides
            // iroh-blobs, so this ceiling can't break a real carrier transfer.
            const MAX_BLOB_CHUNKS: usize = 4096; // 4096 * 180 KB ≈ 720 MB hard ceiling
            let cid = ev.payload.get("cid").and_then(Value::as_str).unwrap_or("").to_string();
            let i = ev.payload.get("i").and_then(Value::as_u64).unwrap_or(0) as usize;
            let n = ev.payload.get("n").and_then(Value::as_u64).unwrap_or(0) as usize;
            let data = ev
                .payload
                .get("b64")
                .and_then(Value::as_str)
                .and_then(|s| B64S.decode(s).ok());
            if cid.is_empty() || n == 0 || n > MAX_BLOB_CHUNKS || i >= n || data.is_none() {
                return false;
            }
            // Cap each chunk so total reassembled size is bounded by n * BLOB_CHUNK
            // (not n * the per-event gossip cap). Legit chunks are <= BLOB_CHUNK.
            if let Some(d) = &data {
                if d.len() > BLOB_CHUNK {
                    return false;
                }
            }
            let now = hey_core::plat::now_ms();
            let complete = {
                let mut p = crate::lock_safe(pending_blobs());
                let entry = p.entry(cid.clone()).or_insert_with(|| (now, vec![None; n]));
                // A re-chunk (different n across senders/versions) resets the buffer
                // instead of deadlocking.
                if entry.1.len() != n {
                    *entry = (now, vec![None; n]);
                }
                entry.1[i] = data;
                entry.1.iter().all(|c| c.is_some())
            };
            if !complete {
                return false;
            }
            let bytes: Vec<u8> = {
                let mut p = crate::lock_safe(pending_blobs());
                match p.remove(&cid) {
                    Some((_, chunks)) => chunks.into_iter().flatten().flatten().collect(),
                    None => return false,
                }
            };
            // Content-address check before storing (reject tampered transfers).
            // Clear the request marker on BOTH paths so a bad transfer can retry.
            crate::lock_safe(requested_blobs()).remove(&cid);
            if format!("b{}", blake3::hash(&bytes).to_hex()) != cid {
                log::warn!("blob {cid}: hash mismatch, dropping");
                return false;
            }
            let _ = content::add_bytes(&bytes, "media", true).await;
            log::info!("fetched media {cid} ({} bytes) over carrier", bytes.len());
            true
        }
        "hey-social.react" => {
            let post_id = ev.payload.get("post_id").and_then(Value::as_str).unwrap_or("");
            let emoji = ev.payload.get("emoji").and_then(Value::as_str).unwrap_or("❤️");
            let unset = ev.payload.get("op").and_then(Value::as_str) == Some("unset");
            if post_id.is_empty() {
                return false;
            }
            // Bind the reaction to the POST AUTHOR's feed topic — react() publishes
            // it there. Without this, any signer on a topic we follow could inject
            // reactions onto arbitrary post_ids in our local view. Drop reactions
            // that arrive on the wrong topic or for a post we don't actually hold.
            match get_post(post_id).await {
                Ok(p) => {
                    let author = p.get("author").and_then(Value::as_str).unwrap_or("");
                    if author.is_empty() || topic != feed_topic(author) {
                        return false;
                    }
                }
                Err(_) => return false,
            }
            let key = format!("hey-social/reactions/{post_id}.json");
            let _g = storage_lock().lock().await;
            let mut map = shared_read_json(&key)
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            // Trust the SIGNER as the reactor (not a payload field).
            if unset {
                map.remove(&ev.sender_did);
            } else {
                map.insert(ev.sender_did.clone(), json!(emoji));
            }
            let _ = shared_write_json(&key, &Value::Object(map)).await;
            true
        }
        "hey-social.comment" => {
            let post_id = ev.payload.get("post_id").and_then(Value::as_str).unwrap_or("");
            let comment = ev.payload.get("comment").cloned().unwrap_or(Value::Null);
            let cauthor = comment.get("author").and_then(Value::as_str).unwrap_or("");
            let cid = comment.get("id").and_then(Value::as_str).unwrap_or("");
            // Comment author must be the signer.
            if post_id.is_empty() || cid.is_empty() || cauthor != ev.sender_did {
                return false;
            }
            // Bind to the POST AUTHOR's feed topic (where add_comment() publishes),
            // and only for posts we hold — otherwise any signer on a followed topic
            // could inject comments onto arbitrary post_ids.
            match get_post(post_id).await {
                Ok(p) => {
                    let author = p.get("author").and_then(Value::as_str).unwrap_or("");
                    if author.is_empty() || topic != feed_topic(author) {
                        return false;
                    }
                }
                Err(_) => return false,
            }
            let key = format!("hey-social/comments/{post_id}.json");
            let _g = storage_lock().lock().await;
            let mut list = shared_read_json(&key)
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            if list.iter().any(|c| c.get("id").and_then(Value::as_str) == Some(cid)) {
                return false; // dedupe
            }
            list.push(comment);
            let _ = shared_write_json(&key, &json!(list)).await;
            true
        }
        "hey-social.sync_req" => {
            // Only the topic OWNER answers (re-announce my recent posts here).
            if topic == feed_topic(me) {
                respond_sync(me).await;
            }
            false
        }
        "hey-social.profile" => {
            // Cache a peer's profile (nickname/bio/avatar/addresses), keyed by signer.
            if topic != feed_topic(&ev.sender_did) {
                return false;
            }
            let path = format!("hey-social/peers/{}.json", ev.sender_did);
            // Freshness gate: never overwrite with an OLDER profile event. Stops a
            // replayed/stale (but validly-signed) profile from rolling back the tip
            // address, and makes broadcast-vs-sync races resolve last-writer-by-ts.
            let prev_ts = shared_read_json(&path).await.ok().flatten()
                .and_then(|v| v.get("ts").and_then(Value::as_i64)).unwrap_or(0);
            if (ev.ts as i64) < prev_ts {
                return false;
            }
            let p = json!({
                "nickname": ev.payload.get("nickname").and_then(Value::as_str).unwrap_or(""),
                "bio": ev.payload.get("bio").and_then(Value::as_str).unwrap_or(""),
                "avatar": ev.payload.get("avatar").and_then(Value::as_str).unwrap_or(""),
                // Tip addresses, authenticated by this event's signature (sender_did).
                "addresses": ev.payload.get("addresses").cloned().unwrap_or(Value::Null),
                "ts": ev.ts,
            });
            let _ = shared_write_json(&path, &p).await;
            // Pull the avatar bytes so it renders.
            if let Some(cid) = ev.payload.get("avatar").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                ensure_blob(cid, &ev.sender_did).await;
            }
            true
        }
        "hey-social.post_delete" => {
            // Author removed a post — remove it on our side too.
            let id = ev.payload.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() {
                return false;
            }
            // Only honor if the post we hold was authored by the signer.
            let existing = shared_read_json(&format!("hey-social/posts/{id}.json")).await.ok().flatten();
            let author = existing.as_ref().and_then(|p| p.get("author")).and_then(Value::as_str);
            if author == Some(ev.sender_did.as_str()) {
                remove_post_local(id).await;
                return true;
            }
            false
        }
        "hey-social.follow" => {
            // Someone (the signer) followed us — record them as a follower + (if
            // they shared keys) bootstrap a DM contact so we can message back.
            // Only meaningful on OUR topic.
            if topic != feed_topic(me) {
                return false;
            }
            let follower = ev.sender_did.clone();
            let ticket = ev.payload.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
            let keys = match (ev.payload.get("x").and_then(Value::as_str), ev.payload.get("k").and_then(Value::as_str)) {
                (Some(x), Some(k)) => Some((x.to_string(), k.to_string())),
                _ => None,
            };
            log::info!(
                "hey-social.follow from {}: has_keys={}",
                follower.chars().take(18).collect::<String>(),
                keys.is_some()
            );
            let changed = {
                let _g = storage_lock().lock().await;
                let mut list = shared_read_json("hey-social/followers.json")
                    .await
                    .ok()
                    .flatten()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                if let Some(existing) =
                    list.iter_mut().find(|e| e.get("did").and_then(Value::as_str) == Some(follower.as_str()))
                {
                    // Already a recorded follower. Refresh keys/ticket if the stored
                    // record lacks them (an earlier keyless follow) so a future boot
                    // reconcile can bootstrap the DM contact from disk.
                    let had_keys = existing.get("x").and_then(Value::as_str).is_some()
                        && existing.get("k").and_then(Value::as_str).is_some();
                    if !had_keys && keys.is_some() {
                        existing["ticket"] = json!(ticket);
                        existing["x"] = json!(keys.as_ref().map(|(x, _)| x.clone()));
                        existing["k"] = json!(keys.as_ref().map(|(_, k)| k.clone()));
                        let _ = shared_write_json("hey-social/followers.json", &json!(list)).await;
                    }
                    false
                } else {
                    list.push(json!({
                        "did": follower, "ticket": ticket, "ts": ev.ts,
                        "x": keys.as_ref().map(|(x, _)| x.clone()),
                        "k": keys.as_ref().map(|(_, k)| k.clone()),
                    }));
                    shared_write_json("hey-social/followers.json", &json!(list)).await.ok();
                    true
                }
            };
            // ALWAYS (re)bootstrap the DM contact when the follow carries keys —
            // not only when the follower is NEWLY recorded. On a re-pair the
            // follower is already in followers.json (changed=false), but the DM
            // contact can be missing (deleted, or an earlier keyless follow);
            // gating bootstrap on `changed` then left the followee subscribed to
            // NO per-pair queue, so the follower's DMs were silently dropped.
            // bootstrap_contact_from_keys is idempotent, so this is safe to run
            // every time.
            if keys.is_some() {
                bootstrap_dm(&follower, &keys, &ticket).await;
            }
            if changed {
                let name = follower.trim_start_matches("did:key:z").chars().take(10).collect::<String>();
                push_notif("follow", &name, "started following you", &follower, &follower);
            }
            changed || keys.is_some()
        }
        _ => false,
    }
}

// ── chat (DMs + groups) — thin wrappers over hey-core's native engine ────────
//
// hey-core's api::dms is fully native (proven by hey-chat-cli); the DM receiver
// is hey_core::peer_receiver::run() (spawned on its own thread in lib.rs). These
// wrappers just ensure a session + marshal to JSON for the Compose UI.

pub async fn chat_contacts() -> Value {
    ensure_session().await.ok();
    let me = whoami_did().await.unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for c in dms::list_contacts().await {
        let mut v = serde_json::to_value(&c).unwrap_or_else(|_| json!({}));
        // Overlay the contact's CURRENT nickname + avatar from their cached profile, so a profile
        // edit shows in the chat list/header (the invite-time name stays as the fallback).
        let prof = raw_profile(&c.did, &me).await;
        if let Some(n) = prof.get("nickname").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            v["name"] = json!(n);
        }
        if let Some(a) = prof.get("avatar").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            v["avatar"] = json!(a);
        }
        out.push(v);
    }
    json!(out)
}
pub async fn chat_groups() -> Value {
    ensure_session().await.ok();
    serde_json::to_value(dms::list_groups().await).unwrap_or_else(|_| json!([]))
}
pub async fn chat_conversation(did: &str) -> Value {
    ensure_session().await.ok();
    let raw = serde_json::to_value(dms::read_conversation(did).await).unwrap_or_else(|_| json!([]));
    // Cache any address card the peer DM'd us (so we can tip them by identity) + strip
    // those hidden control messages from the thread; then ensure they have mine.
    let arr = raw.as_array().cloned().unwrap_or_default();
    let deleted = collect_deleted(&arr);
    let edits = collect_edits(&arr);
    let mut out: Vec<Value> = Vec::new();
    for m in arr {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
        let id = m.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        if let Some(b64) = text.strip_prefix(ADDR_PREFIX) {
            if !mine {
                if let Ok(bytes) = B64U.decode(b64.trim()) {
                    if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                        cache_peer_addresses(did, v).await;
                    }
                }
            }
            continue; // hidden control message — never shown in the chat
        }
        if text.starts_with(CALL_PREFIX) || text.starts_with(DEL_PREFIX) || text.starts_with(EDIT_PREFIX) {
            continue; // hidden control messages — handled elsewhere, never shown
        }
        if deleted.contains(&(id, mine)) {
            continue; // deleted by its sender
        }
        out.push(apply_edit(m, &edits));
    }
    let _ = share_addresses(did).await; // once per peer; opening the chat bootstraps it
    json!(out)
}
pub async fn chat_group_conversation(gid: &str) -> Value {
    ensure_session().await.ok();
    let raw = serde_json::to_value(dms::read_group_conversation(gid).await).unwrap_or_else(|_| json!([]));
    let arr = raw.as_array().cloned().unwrap_or_default();
    let deleted = collect_deleted(&arr);
    let edits = collect_edits(&arr);
    // Hide control messages (group-call / delete / edit); drop deleted; apply edits.
    let out: Vec<Value> = arr
        .into_iter()
        .filter(|m| {
            let text = m.get("text").and_then(Value::as_str).unwrap_or("");
            if text.starts_with(GCALL_PREFIX) || text.starts_with(DEL_PREFIX) || text.starts_with(EDIT_PREFIX) {
                return false;
            }
            let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
            let id = m.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            !deleted.contains(&(id, mine))
        })
        .map(|m| apply_edit(m, &edits))
        .collect();
    json!(out)
}
pub async fn chat_send(did: &str, text: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    dms::send_message(did, text)
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}

/// A contact's carrier ticket (base32 EndpointAddr) — used to dial their `hey/voice/1` ALPN for a
/// voice call. Empty if unknown (same-runtime/legacy contact).
pub async fn peer_ticket(did: &str) -> String {
    dms::list_contacts()
        .await
        .into_iter()
        .find(|c| c.did == did)
        .and_then(|c| c.peer_ticket)
        .unwrap_or_default()
}

/// Live transport to ONE contact: "direct" | "relay" | "offline". Resolves the
/// stored ticket then asks the carrier — the per-peer source of truth for the
/// call direct-gate (the node-level `direct` from carrier_health is true if ANY
/// peer is direct, which is wrong for a specific contact).
pub async fn contact_transport(did: &str) -> String {
    let ticket = peer_ticket(did).await;
    if ticket.is_empty() {
        return "offline".into();
    }
    if let Some((_h, slot)) = crate::NET.get() {
        if let Some(c) = slot.read().await.clone() {
            return c.peer_transport(&ticket).await.to_string();
        }
    }
    "offline".into()
}

// ── DDRM encrypted 3D assets (.ddrm) — the easy half of the HTKS path ─────────
//
// A `.ddrm` is just Hey's own ChaCha20-Poly1305-sealed `.glb` (reuses crypto's
// at-rest sealer), stored via the content/blobs provider. `ddrm_pack` is the
// creator side; `ddrm_load` is the on-device owner side — fetch + decrypt IN
// MEMORY (never written to disk = "never take the file") for the Verse Godot
// loader (GLTFDocument.append_from_buffer). The content key `ck` is supplied by
// the caller for now; the HTKS t-of-n-across-relays release slots in underneath
// later WITHOUT changing this decrypt path. See docs/HEY_THRESHOLD_KEY_SERVICE.md.

fn parse_ck(ck_b64: &str) -> Result<[u8; 32], String> {
    let v = B64S.decode(ck_b64.trim()).map_err(|_| "ddrm ck: bad base64".to_string())?;
    v.try_into().map_err(|_| "ddrm ck: key must be exactly 32 bytes".to_string())
}

/// Encrypt asset bytes (a `.glb`) with a 32-byte content key (base64) → a `.ddrm`
/// blob; store it via the content provider; return its cid.
pub async fn ddrm_pack(glb: &[u8], ck_b64: &str) -> Result<String, String> {
    let ck = parse_ck(ck_b64)?;
    let ddrm = hey_core::crypto::seal_at_rest(&ck, glb);
    let resp = content::add_bytes(&ddrm, "ddrm", true).await.map_err(|e| e.to_string())?;
    content::extract_cid(&resp).ok_or_else(|| "ddrm_pack: provider returned no cid".to_string())
}

/// Fetch a `.ddrm` blob by cid + decrypt with the content key → plaintext `.glb`
/// bytes, in memory. `Err` if the blob is missing or the key is wrong/tampered.
pub async fn ddrm_load(cid: &str, ck_b64: &str) -> Result<Vec<u8>, String> {
    let ck = parse_ck(ck_b64)?;
    let ddrm = content::get_bytes(cid, None).await.map_err(|e| e.to_string())?;
    hey_core::crypto::open_at_rest(&ck, &ddrm)
        .ok_or_else(|| "ddrm_load: decrypt failed (wrong key or tampered blob)".to_string())
}

/// `ddrm_load` + base64 the plaintext `.glb` for the JNI→Godot string boundary
/// (Godot decodes with `Marshalls.base64_to_raw` → `GLTFDocument.append_from_buffer`).
pub async fn ddrm_load_b64(cid: &str, ck_b64: &str) -> Result<String, String> {
    Ok(B64S.encode(ddrm_load(cid, ck_b64).await?))
}

/// `ddrm_pack` taking the `.glb` as base64 (GDScript reads `res://…glb` → base64) → cid.
pub async fn ddrm_pack_b64(glb_b64: &str, ck_b64: &str) -> Result<String, String> {
    let glb = B64S.decode(glb_b64.trim()).map_err(|_| "ddrm: bad glb base64".to_string())?;
    ddrm_pack(&glb, ck_b64).await
}
/// Download progress 0..=100 for an in-flight attachment fetch, -1 if not active. Sync.
pub fn attachment_progress(id: &str) -> i32 {
    dms::attachment_progress(id)
}
pub async fn chat_send_group(gid: &str, text: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    dms::send_group_message(gid, text)
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}
pub async fn chat_unread() -> u32 {
    dms::total_unread().await
}
pub async fn chat_mark_read(did: &str) {
    dms::mark_read(did).await;
}
pub async fn chat_mark_group_read(gid: &str) {
    dms::mark_group_read(gid).await;
}
pub async fn chat_gen_invite(label: &str) -> Result<String, String> {
    ensure_session().await.ok();
    dms::generate_invite(label, dms::IdentityMode::Regular).await
}
pub async fn chat_accept_invite(token: &str) -> Result<String, String> {
    ensure_session().await.ok();
    dms::accept_invite(token, dms::IdentityMode::Regular).await
}

// ── attachments (files/photos in DMs + groups) ───────────────────────────────
//
// hey-core does the E2E heavy lifting (encrypt per-file key, inline ≤16KB over
// the carrier else chunk to the content store). We just upload then send the ref.

/// Send a 1-to-1 DM carrying one attachment (uploaded + sealed by hey-core).
pub async fn chat_send_attachment(
    did: &str,
    text: &str,
    bytes: &[u8],
    mime: &str,
    filename: &str,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let att = dms::upload_attachment(filename, mime, bytes).await?;
    dms::send_message_with_attachments(did, text, vec![att])
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}

/// Send a group message carrying one attachment.
pub async fn chat_send_group_attachment(
    gid: &str,
    text: &str,
    bytes: &[u8],
    mime: &str,
    filename: &str,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let att = dms::upload_attachment(filename, mime, bytes).await?;
    dms::send_group_message_with_attachments(gid, text, vec![att])
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}

/// Fetch + decrypt one attachment's plaintext bytes (for render/download). The
/// UI passes the Attachment JSON straight from the message it came in on.
pub async fn chat_fetch_attachment(att_json: &str) -> Result<Vec<u8>, String> {
    ensure_session().await.ok();
    let att: dms::Attachment =
        serde_json::from_str(att_json).map_err(|e| format!("parse attachment: {e}"))?;
    dms::fetch_attachment(&att).await
}

/// Streamed (torrent-style) send: the file is read from `path` and uploaded
/// chunk-by-chunk (O(chunk) RAM) — big files don't OOM. Small files fall back to
/// the bytes path inside upload_attachment_streaming.
pub async fn chat_send_attachment_path(
    did: &str,
    text: &str,
    path: &str,
    mime: &str,
    filename: &str,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let att = dms::upload_attachment_streaming(path, filename, mime).await?;
    dms::send_message_with_attachments(did, text, vec![att])
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}

pub async fn chat_send_group_attachment_path(
    gid: &str,
    text: &str,
    path: &str,
    mime: &str,
    filename: &str,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let att = dms::upload_attachment_streaming(path, filename, mime).await?;
    dms::send_group_message_with_attachments(gid, text, vec![att])
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}

/// Streamed (torrent-style) fetch: download + decrypt the attachment chunk-by-chunk
/// straight to `dest` on disk (O(chunk) RAM). Returns {"ok":true} on success.
pub async fn chat_fetch_attachment_to_path(att_json: &str, dest: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    let att: dms::Attachment =
        serde_json::from_str(att_json).map_err(|e| format!("parse attachment: {e}"))?;
    dms::fetch_attachment_to_path(&att, dest).await?;
    Ok(serde_json::json!({ "ok": true }))
}

// ── group create ─────────────────────────────────────────────────────────────

/// Create a group from a JSON array of member DIDs (must be active contacts).
/// Returns `{ "id": "<group id>" }`.
pub async fn chat_create_group(name: &str, members_json: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    let members: Vec<String> =
        serde_json::from_str(members_json).map_err(|e| format!("parse members: {e}"))?;
    dms::create_group(name, members)
        .await
        .map(|id| json!({ "id": id }))
}

// ── per-message reactions ────────────────────────────────────────────────────

/// Toggle MY reaction on a message (DM or group). Returns my resulting emoji.
pub async fn chat_react_message(
    chat_id: &str,
    message_id: &str,
    emoji: &str,
    is_group: bool,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let mine = if is_group {
        dms::send_group_message_reaction(chat_id, message_id, emoji).await?
    } else {
        dms::send_message_reaction(chat_id, message_id, emoji).await?
    };
    Ok(json!({ "emoji": mine }))
}

/// All reactions in a conversation (DM or group), as a JSON array the UI groups
/// by `message_id`.
pub async fn chat_message_reactions(chat_id: &str, is_group: bool) -> Value {
    ensure_session().await.ok();
    let list = if is_group {
        dms::read_group_reactions(chat_id).await
    } else {
        dms::read_dm_reactions(chat_id).await
    };
    serde_json::to_value(list).unwrap_or_else(|_| json!([]))
}

// ── media transfer over the carrier (photo bytes follow the post) ────────────

const BLOB_CHUNK: usize = 180_000; // raw bytes/chunk; b64 + event JSON < 1 MB gossip cap
const BLOB_RETRY_MS: i64 = 30_000; // re-request a missing blob at most this often
const PENDING_TTL_MS: i64 = 90_000; // drop half-received chunk sets after this

// cid -> (inserted_ms, slots). Evicted on completion OR by TTL in poll_once.
fn pending_blobs() -> &'static Mutex<HashMap<String, (i64, Vec<Option<Vec<u8>>>)>> {
    static P: OnceLock<Mutex<HashMap<String, (i64, Vec<Option<Vec<u8>>>)>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}
// cid -> last_request_ms (so a failed transfer self-heals instead of poisoning).
fn requested_blobs() -> &'static Mutex<HashMap<String, i64>> {
    static R: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// If we don't have `cid` locally, ask its author for it (rate-limited retry).
async fn ensure_blob(cid: &str, author: &str) {
    if cid.is_empty() || content::get_bytes(cid, None).await.is_ok() {
        return;
    }
    let now = hey_core::plat::now_ms();
    {
        let mut r = crate::lock_safe(requested_blobs());
        if let Some(&last) = r.get(cid) {
            if now - last < BLOB_RETRY_MS {
                return;
            }
        }
        r.insert(cid.to_string(), now);
    }
    publish(&feed_topic(author), "hey-social.blob_req", json!({ "cid": cid })).await;
}

/// Chunk + broadcast a blob we hold, in response to a blob_req on `topic`.
async fn send_blob(cid: &str, bytes: &[u8], topic: &str) {
    let n = (bytes.len() + BLOB_CHUNK - 1) / BLOB_CHUNK;
    for i in 0..n {
        let start = i * BLOB_CHUNK;
        let end = ((i + 1) * BLOB_CHUNK).min(bytes.len());
        let b64 = B64S.encode(&bytes[start..end]);
        publish(topic, "hey-social.blob", json!({ "cid": cid, "i": i, "n": n, "b64": b64 })).await;
    }
}

/// Re-broadcast my recent posts on my topic so a freshly-joined follower
/// backfills. Idempotent on the receiver (dedupe by post id).
async fn respond_sync(me: &str) {
    let idx: Vec<Value> = shared_read_json(FEED_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut sent = 0;
    for id in idx.iter().filter_map(Value::as_str) {
        if sent >= 30 {
            break;
        }
        if let Ok(Some(p)) = shared_read_json(&format!("hey-social/posts/{id}.json")).await {
            if p.get("author").and_then(Value::as_str) == Some(me) {
                publish(&feed_topic(me), "hey-social.post", p).await;
                sent += 1;
            }
        }
    }
}
