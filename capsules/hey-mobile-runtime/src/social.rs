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

// ── feed / comments storage caps (unbounded-growth DoS guards) ───────────────
// The feed index + per-post comment lists grow from ingested remote events, so a
// peer flooding posts/comments on a followed topic could grow them without bound.
// Cap each; eviction of the OLDEST feed entry also deletes its backing
// posts/{id}.json (+ reactions/comments) so disk doesn't leak. These ceilings are
// far above any real timeline, so they never trim a legitimate feed.
const FEED_INDEX_CAP: usize = 5000; // max posts retained in the local timeline
const MAX_COMMENTS_PER_POST: usize = 1000; // max comments stored per post
const MAX_INGEST_PAYLOAD: usize = 256 * 1024; // reject a single ingested event larger than this

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
                    // PERSISTENCE across restart: a relaunch must keep my real
                    // nickname so the handshake + every "sn" carry it (not the
                    // "hey-XXXXXX" fallback). The session name can come back empty
                    // after a wipe/upgrade, so re-seed it from the stored social
                    // profile whenever it's missing/placeholder.
                    reconcile_session_name(&s).await;
                    repair_contacts_once().await; // Fix 2: once-per-boot self-repair sweep
                    return Ok(did);
                }
                log::warn!(
                    "session did_key {} != identity {} — resetting session to identity",
                    s.did_key, did
                );
            }
            // Seed the new session's name from the stored social profile (if any),
            // so the chat engine has my real nickname immediately on boot.
            let name = stored_profile_nickname().await;
            session::set(&session::Session {
                auth_key_hex: String::new(),
                did_key: did.clone(),
                name,
                ml_kem_secret_b64: String::new(),
                ml_kem_public_b64: String::new(),
            });
            repair_contacts_once().await; // Fix 2: once-per-boot self-repair sweep
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

/// Fix 2: run the contact self-repair sweep at most ONCE per process boot. Guard
/// with an atomic compare_exchange so the many ensure_session() calls per session
/// don't re-sweep every contact each time.
async fn repair_contacts_once() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RAN: AtomicBool = AtomicBool::new(false);
    if RAN
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        dms::repair_all_contacts().await;
    }
}

/// My chosen nickname from the stored social profile (`hey-social/profile.json`),
/// or empty if none/placeholder. Source of truth for the session display name.
async fn stored_profile_nickname() -> String {
    my_profile()
        .await
        .get("nickname")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty() && !dms::is_generated_label(n))
        .map(str::to_string)
        .unwrap_or_default()
}

/// Keep the session display name in sync with the stored social nickname across a
/// relaunch: if the session name is missing/placeholder but the profile holds a
/// real nickname, re-point the session at it (so the chat engine's handshake +
/// every "sn" carry the real name, not "hey-XXXXXX").
async fn reconcile_session_name(s: &session::Session) {
    if !dms::is_generated_label(&s.name) {
        return; // already a real name
    }
    let nick = stored_profile_nickname().await;
    if !nick.is_empty() && nick != s.name {
        let mut s2 = s.clone();
        s2.name = nick;
        session::set(&s2);
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
    // Set-backed dedup (O(1) membership instead of an O(n) scan per insert).
    let seen: std::collections::HashSet<&str> = idx.iter().filter_map(Value::as_str).collect();
    if seen.contains(id) {
        return;
    }
    drop(seen);
    idx.insert(0, json!(id));
    // Cap the timeline: drop the OLDEST ids past the ceiling and delete their
    // backing posts/reactions/comments so disk doesn't leak. Backward-compatible:
    // a pre-existing over-cap index is trimmed on the next insert.
    if idx.len() > FEED_INDEX_CAP {
        let evicted: Vec<String> = idx
            .split_off(FEED_INDEX_CAP)
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        for old in evicted {
            let _ = shared_write_json(&format!("hey-social/posts/{old}.json"), &Value::Null).await;
            let _ = shared_write_json(&format!("hey-social/reactions/{old}.json"), &Value::Null).await;
            let _ = shared_write_json(&format!("hey-social/comments/{old}.json"), &Value::Null).await;
            // Replay-gate sidecars (per-reactor / per-author high-water ts). Delete them WITH the
            // post so a Sybil (one signed reaction/comment per minted DID) can't grow orphaned
            // sidecars unbounded after the post itself is evicted. Same Null-write delete idiom.
            let _ = shared_write_json(&format!("hey-social/reactions/{old}.seen.json"), &Value::Null).await;
            let _ = shared_write_json(&format!("hey-social/comments/{old}.seen.json"), &Value::Null).await;
        }
    }
    let _ = shared_write_json(FEED_INDEX, &json!(idx)).await;
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
    // Compact the advertised ticket (<=4 direct-IP hints + ALL relays) so the
    // call lane that fans this into GCALL/call payloads never leaks the full
    // uncompacted node ticket. Stays dialable: peers connect via relay, then
    // iroh upgrades to a direct path — matching the follow/friend-link lane.
    let ticket = compact_ticket(&runtime::peer::my_ticket().await.unwrap_or_default());
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

// ── PRIVATE FEED ENCRYPTION ──────────────────────────────────────────────────
// Posts ride the author's gossip topic SEALED: only an ACCEPTED follower — handed the per-epoch
// feed key over a sealed DM — can open them. The seal/unseal happens ONLY at the network boundary:
// every OUTBOUND post (create / edit / backfill) is sealed under MY CURRENT epoch, and an INBOUND
// post is decrypted at ingest and stored PLAINTEXT (the on-disk store is already DEK-sealed at
// rest), so the read/render paths are unchanged. Removing a follower bumps the epoch and re-keys
// the remaining followers, so the removed follower's key can't open any new (or re-broadcast) post.
// Backward compatible: a post that arrives with no `sealed` field is ingested as cleartext.

/// My current feed epoch (author side). Bumped on follower removal for forward-secrecy.
async fn my_feed_epoch() -> u32 {
    shared_read_json("hey-social/feed-state.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.get("epoch").and_then(Value::as_u64))
        .unwrap_or(0) as u32
}

async fn set_my_feed_epoch(epoch: u32) {
    let _ = shared_write_json("hey-social/feed-state.json", &json!({ "epoch": epoch })).await;
}

/// My symmetric feed key for `epoch`, via the CONFINED identity accessor (the raw seed never
/// leaves identity.rs; this is a one-way HKDF derivative, epoch-scoped).
fn my_feed_key(epoch: u32) -> Option<zeroize::Zeroizing<[u8; 32]>> {
    crate::IDENTITY.get().map(|id| id.feed_key(epoch))
}

/// Seal an outbound post under my CURRENT epoch → the cleartext routing envelope that rides the
/// topic: {id, author, ts, epoch, sealed, enc}. The WHOLE post JSON (caption/media/author_name/
/// avatar) is the sealed plaintext, so a non-follower learns only that a post exists + its
/// id/author/ts (needed for dedup + author==signer validation). If no identity is loaded (never on
/// a real device) it returns the post unsealed so a post is never silently dropped.
async fn seal_post_outbound(post: &Value) -> Value {
    let epoch = my_feed_epoch().await;
    let Some(key) = my_feed_key(epoch) else { return post.clone() };
    let sealed = hey_core::crypto::seal_feed_post(&key, epoch, post.to_string().as_bytes());
    json!({
        "id": post.get("id").cloned().unwrap_or(Value::Null),
        "author": post.get("author").cloned().unwrap_or(Value::Null),
        "ts": post.get("ts").cloned().unwrap_or(Value::Null),
        "epoch": epoch,
        "sealed": sealed,
        "enc": true,
    })
}

/// A feed key I hold for `author` at `epoch` (follower side), or None.
async fn feed_key_for(author: &str, epoch: u32) -> Option<[u8; 32]> {
    let map = shared_read_json(&format!("hey-social/feed-keys/{author}.json")).await.ok().flatten()?;
    let b64 = map.get(epoch.to_string()).and_then(Value::as_str)?;
    let raw = B64S.decode(b64).ok()?;
    <[u8; 32]>::try_from(raw.as_slice()).ok()
}

/// Open an ingested sealed post envelope → the full plaintext post. None when it isn't a sealed
/// post (caller treats it as legacy cleartext) OR I hold no key for its epoch (caller drops it; a
/// later key + backfill re-delivers it). Binds the sealed content to the routing envelope: the
/// decrypted author/id MUST match the cleartext envelope, so a sealed blob can't be replayed
/// under a different envelope.
async fn open_sealed_post(env: &Value) -> Option<Value> {
    let sealed = env.get("sealed").and_then(Value::as_str)?;
    let author = env.get("author").and_then(Value::as_str)?;
    let epoch = hey_core::crypto::feed_post_epoch(sealed)?;
    let key = feed_key_for(author, epoch).await?;
    let pt = hey_core::crypto::open_feed_post(&key, sealed)?;
    let post: Value = serde_json::from_str(&pt).ok()?;
    if post.get("author").and_then(Value::as_str) != Some(author) {
        return None;
    }
    if post.get("id") != env.get("id") {
        return None;
    }
    Some(post)
}

/// Cache a feed key received from `author` (the VERIFIED DM sender) into the per-author epoch→key
/// map, then pull a backfill so any posts that arrived before the key now decrypt. Idempotent.
async fn cache_feed_key(author: &str, epoch: u32, key_b64: &str) {
    if B64S.decode(key_b64).ok().map(|b| b.len()) != Some(32) {
        return; // not a real 32-byte key
    }
    let newly_added;
    {
        let _g = storage_lock().lock().await;
        let key = format!("hey-social/feed-keys/{author}.json");
        let mut map = shared_read_json(&key).await.ok().flatten().unwrap_or_else(|| json!({}));
        if !map.is_object() {
            map = json!({});
        }
        let ek = epoch.to_string();
        newly_added = map.get(&ek).and_then(Value::as_str) != Some(key_b64);
        if newly_added {
            map[ek] = json!(key_b64);
            let _ = shared_write_json(&key, &map).await;
        }
    }
    if newly_added {
        // Posts that arrived before the key were dropped; respond_sync re-seals under the author's
        // CURRENT epoch, so a backfill now decrypts.
        let _ = runtime::peer::join_topic(&feed_topic(author)).await;
        publish(&feed_topic(author), "hey-social.sync_req", json!({ "want": "backfill" })).await;
    }
}

/// Send my CURRENT feed key to `to_did` — but ONLY if they are actually a follower of mine (never
/// hand feed-read capability to a non-follower) and the DM channel isn't verify-gated (same fail-
/// closed rule as the wallet address card: don't seal to attacker-substitutable keys). De-duped
/// per (follower, epoch) so chat-open retries don't re-send. Returns whether the key is delivered.
async fn send_feed_key_to(to_did: &str) -> bool {
    if to_did.is_empty() {
        return false;
    }
    let me = whoami_did().await.unwrap_or_default();
    if to_did == me || me.is_empty() {
        return false;
    }
    let followers = shared_read_json("hey-social/followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    // Must be an ACCEPTED follower: present AND not pending. Defense-in-depth — never hand
    // feed-read capability to a non-follower OR a not-yet-accepted (pending) request, regardless of
    // caller. accept_follower clears pending BEFORE calling this; rekey/retry pre-filter too.
    let Some(entry) = followers.iter().find(|e| e.get("did").and_then(Value::as_str) == Some(to_did))
    else {
        return false; // not my follower
    };
    if entry.get("pending").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    match dms::find_contact(to_did).await {
        None => return false,
        Some(c) if c.needs_verify_before_send || c.key_changed => {
            return false; // verify-gated / key-changed → hold the key until re-verified
        }
        Some(_) => {}
    }
    let epoch = my_feed_epoch().await;
    let now_ms = hey_core::plat::now_ms() as u64;
    // Suppress re-send when delivery is CONFIRMED by the receiver's ACK for THIS epoch, OR when we're
    // still inside the per-(follower,epoch) backoff window. The acked check fixes the "stuck on
    // Requested" bug (a bare chat_send Ok is fire-and-forget, NOT proof of delivery, so it must not
    // permanently dedupe). The backoff check rate-limits EVERY caller — incl. the chat-open retry that
    // fires every ~2s — so an undeliverable key can never storm. retry_pending_feed_keys re-drives a
    // non-acked record once its next_ms elapses. (Old bare-epoch-scalar records have no "acked"/
    // "next_ms" → re-sent immediately, which is exactly the recovery we want.)
    {
        let _g = storage_lock().lock().await;
        let sent = shared_read_json("hey-social/feed-key-sent.json").await.ok().flatten().unwrap_or_else(|| json!({}));
        if let Some(rec) = sent.get(to_did) {
            if rec.get("epoch").and_then(Value::as_u64) == Some(epoch as u64) {
                if rec.get("acked").and_then(Value::as_bool) == Some(true) {
                    return true; // delivery CONFIRMED by receiver ACK
                }
                // Not acked yet, but still within the backoff window → don't re-send (rate-limit).
                if rec.get("next_ms").and_then(Value::as_u64).is_some_and(|n| now_ms < n) {
                    return true;
                }
            }
        }
    }
    let Some(key) = my_feed_key(epoch) else {
        return false;
    };
    let payload = json!({ "author": me, "epoch": epoch, "key": B64S.encode(&key[..]) });
    let b64 = B64U.encode(payload.to_string().as_bytes());
    let ok = chat_send(to_did, &format!("{FEED_KEY_PREFIX}{b64}")).await.is_ok();
    // Persist the attempt record on BOTH ok AND err. A fire-and-forget chat_send Ok isn't proof of
    // delivery, and an Err (session lost mid-flight / queue not ready / no peer keys yet) must STILL
    // advance attempts+backoff — otherwise retry_pending_feed_keys would re-drive a persistently-
    // broken channel every 2s with NO backoff and never hit ATTEMPT_CAP (a mini-loop). Recording it
    // rate-limits the retry and lets the cap eventually stop a dead channel; a recovered channel just
    // retries after the backoff elapses.
    {
        let _g = storage_lock().lock().await;
        let mut sent = shared_read_json("hey-social/feed-key-sent.json").await.ok().flatten().unwrap_or_else(|| json!({}));
        if !sent.is_object() {
            sent = json!({});
        }
        let prev = sent.get(to_did);
        let same_epoch = prev.and_then(|r| r.get("epoch")).and_then(Value::as_u64) == Some(epoch as u64);
        let attempts = if same_epoch {
            prev.and_then(|r| r.get("attempts")).and_then(Value::as_u64).unwrap_or(0) + 1
        } else {
            1
        };
        let acked = same_epoch && prev.and_then(|r| r.get("acked")).and_then(Value::as_bool).unwrap_or(false);
        let now = hey_core::plat::now_ms() as u64;
        // Exponential backoff: ~5s, 10s, 20s … capped at 5min. Shift clamped to avoid overflow.
        let backoff = std::cmp::min(300_000u64, 5_000u64.saturating_mul(1u64 << attempts.min(6)));
        sent[to_did] = json!({ "epoch": epoch, "attempts": attempts, "next_ms": now + backoff, "acked": acked });
        let _ = shared_write_json("hey-social/feed-key-sent.json", &sent).await;
    }
    ok
}

/// Re-drive feed-key delivery to every ACCEPTED follower whose key isn't yet ACK-confirmed, honoring
/// per-follower backoff + an attempt cap. Driven from poll_once. Idempotent on the receiver (dedup by
/// message id + key-cache compare), so safe to call every tick; recovers an accepted-but-unkeyed
/// follower even across an app restart (state is durable in followers.json + feed-key-sent.json).
async fn retry_pending_feed_keys() {
    const ATTEMPT_CAP: u64 = 40; // mirrors PENDING_BROADCAST_ATTEMPTS
    let epoch = my_feed_epoch().await as u64;
    let now = hey_core::plat::now_ms() as u64;
    let followers = shared_read_json("hey-social/followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let sent = shared_read_json("hey-social/feed-key-sent.json").await.ok().flatten().unwrap_or_else(|| json!({}));
    for f in followers {
        // Skip pending (not yet accepted). send_feed_key_to re-checks membership + verify-gate +
        // pending, so this is a cheap pre-filter; the authoritative gates live there.
        if f.get("pending").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(did) = f.get("did").and_then(Value::as_str) else { continue };
        let due = match sent.get(did) {
            None => true, // never sent
            Some(r) => {
                let same = r.get("epoch").and_then(Value::as_u64) == Some(epoch);
                let acked = r.get("acked").and_then(Value::as_bool).unwrap_or(false);
                let attempts = r.get("attempts").and_then(Value::as_u64).unwrap_or(0);
                let next = r.get("next_ms").and_then(Value::as_u64).unwrap_or(0);
                !same /* epoch rotated → must re-send */
                    || (!acked && now >= next && attempts < ATTEMPT_CAP)
            }
        };
        if due {
            send_feed_key_to(did).await;
        }
    }
}

/// Rotate my feed epoch after removing a follower and re-key every REMAINING (accepted) follower,
/// so the removed follower's old key can't open any subsequent (or re-broadcast) post. Caller MUST
/// NOT hold storage_lock (send_feed_key_to re-acquires it).
async fn rekey_remaining_followers() {
    let next = my_feed_epoch().await.wrapping_add(1);
    set_my_feed_epoch(next).await;
    let followers = shared_read_json("hey-social/followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    for e in followers.iter() {
        if e.get("pending").and_then(Value::as_bool).unwrap_or(false) {
            continue; // pending followers were never keyed
        }
        if let Some(d) = e.get("did").and_then(Value::as_str) {
            send_feed_key_to(d).await;
        }
    }
}

// ── feed content sealing for COMMENTS / REACTIONS / PROFILE ──────────────────
// These ride the POST AUTHOR's topic and are sealed under the AUTHOR's feed key — exactly like
// posts. A commenter/reactor is a follower who HOLDS the author's key, so they can seal; the author
// derives their own key. So the same "only accepted followers can read" property extends to all
// feed activity, not just post bodies.

/// The epoch to seal NEW content under for `author`'s feed: my current epoch if I'm the author,
/// else the highest epoch key I hold for them (= their current epoch). None if I hold no key.
async fn author_current_epoch(author: &str, me: &str) -> Option<u32> {
    if author == me {
        return Some(my_feed_epoch().await);
    }
    let map = shared_read_json(&format!("hey-social/feed-keys/{author}.json")).await.ok().flatten()?;
    map.as_object()?.keys().filter_map(|k| k.parse::<u32>().ok()).max()
}

/// A feed key for `author` at `epoch`: derived from my seed if I'm the author, else from my stored
/// per-author map (the key they handed me on accept).
async fn feed_key_any(author: &str, me: &str, epoch: u32) -> Option<zeroize::Zeroizing<[u8; 32]>> {
    if author == me {
        my_feed_key(epoch)
    } else {
        feed_key_for(author, epoch).await.map(zeroize::Zeroizing::new)
    }
}

/// Seal `content` under `author`'s current feed epoch → (epoch, sealed_b64). None when I hold no
/// key for that author — the caller then skips the broadcast rather than leak cleartext.
async fn seal_for_feed(author: &str, content: &Value) -> Option<(u32, String)> {
    let me = whoami_did().await.unwrap_or_default();
    let epoch = author_current_epoch(author, &me).await?;
    let key = feed_key_any(author, &me, epoch).await?;
    Some((epoch, hey_core::crypto::seal_feed_post(&key, epoch, content.to_string().as_bytes())))
}

/// Open feed content sealed under `author`'s feed key (picks the key by the epoch in the blob).
async fn open_for_feed(author: &str, sealed: &str) -> Option<Value> {
    let me = whoami_did().await.unwrap_or_default();
    let epoch = hey_core::crypto::feed_post_epoch(sealed)?;
    let key = feed_key_any(author, &me, epoch).await?;
    let pt = hey_core::crypto::open_feed_post(&key, sealed)?;
    serde_json::from_str(&pt).ok()
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
        // Monotonic revision for the ingest freshness gate (bumped by edit_post). A replayed older
        // event then can't roll back a newer edit.
        "edited_ts": ts,
    });
    // Store the post + index it, then broadcast it on my author topic so
    // followers receive it over the carrier.
    shared_write_json(&format!("hey-social/posts/{id}.json"), &post)
        .await
        .map_err(|e| format!("store post: {e}"))?;
    add_to_index(&id).await;
    // PRIVATE FEED: store plaintext locally (DEK-sealed at rest) but broadcast SEALED to the
    // topic — only accepted followers holding my current epoch key can open it.
    let wire = seal_post_outbound(&post).await;
    publish(&feed_topic(&did), "hey-social.post", wire).await;
    Ok(post)
}

/// My profile (nickname/bio/avatar), or empty defaults.
async fn my_profile() -> Value {
    shared_read_json("hey-social/profile.json").await.ok().flatten().unwrap_or_else(|| json!({}))
}

/// Set/update my profile, persist it, and broadcast it so followers re-cache.
pub async fn set_profile(nickname: &str, bio: &str, avatar: &str) -> Result<Value, String> {
    let did = whoami_did().await?;
    // Serialize the profile.json read-modify-write (set_profile preserves addresses, set_tip_addresses
    // preserves nickname/bio/avatar) so two concurrent edits can't clobber each other's field.
    let _g = storage_lock().lock().await;
    // Preserve any published tip addresses across a profile edit.
    let addresses = my_profile().await.get("addresses").cloned().unwrap_or(Value::Null);
    let p = json!({ "did": did, "nickname": nickname, "bio": bio, "avatar": avatar, "addresses": addresses, "ts": hey_core::plat::now_ms() });
    shared_write_json("hey-social/profile.json", &p).await.map_err(|e| e.to_string())?;
    // ROOT FIX: the chat engine reads MY display name off the hey-core session
    // (ensure_profile().name → the handshake "name" + every 1:1/group "sn"). The
    // social nickname lives in profile.json, so mirror it into the session here —
    // otherwise session.name stays empty and chat falls back to "hey-XXXXXX".
    if let Some(mut s) = hey_core::session::current() {
        if s.name != nickname {
            s.name = nickname.to_string();
            hey_core::session::set(&s);
        }
    }
    // Push the new name to followers (feed) AND straight to every chat/group
    // contact so they refresh immediately, not on my next message.
    broadcast_profile(&did, &p).await;
    dms::broadcast_profile_name(nickname).await;
    Ok(p)
}

/// Record my tip-receive addresses (a `{chainKey: address}` map) into my LOCAL
/// profile only. SECURITY (F-04): broadcast_profile DELIBERATELY omits `addresses`
/// from the public DID-derived feed topic — they are NOT published to followers.
/// They reach contacts solely over the sealed end-to-end DM
/// (share_addresses / cache_peer_addresses), so a peer can tip me by identity
/// without the map ever leaking onto the public feed.
pub async fn set_tip_addresses(addresses_json: &str) -> Result<Value, String> {
    let did = whoami_did().await?;
    // Serialize the profile.json RMW (see set_profile) so a concurrent profile edit isn't lost.
    let _g = storage_lock().lock().await;
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

/// Broadcast the public profile (nickname/bio/avatar) so followers re-cache it.
/// SECURITY (F-04): the on-chain wallet `addresses` map is DELIBERATELY omitted —
/// it must never ride the public DID-derived feed topic. Addresses reach contacts
/// only over the sealed end-to-end DM (share_addresses / cache_peer_addresses).
async fn broadcast_profile(did: &str, p: &Value) {
    // PRIVATE FEED: seal my profile (nickname/bio/avatar) under my current feed key — only accepted
    // followers can read it (the chat-name path is seeded separately via the follow announce + the
    // denormalized author_name inside posts, so chat display is unaffected). `did` is always me here
    // (set_profile / respond_sync), so seal_for_feed uses my current epoch. Skip if no key.
    let content = json!({
        "nickname": p.get("nickname").and_then(Value::as_str).unwrap_or(""),
        "bio": p.get("bio").and_then(Value::as_str).unwrap_or(""),
        "avatar": p.get("avatar").and_then(Value::as_str).unwrap_or(""),
    });
    if let Some((epoch, sealed)) = seal_for_feed(did, &content).await {
        publish(
            &feed_topic(did),
            "hey-social.profile",
            json!({ "epoch": epoch, "sealed": sealed, "enc": true }),
        )
        .await;
    }
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
/// F-FOLLOW-ANNOUNCE: hidden follow key-bundle, base64(json {ticket,name,x,k}),
/// over the SAME SOH-prefixed sealed E2E-DM channel. Replaces the cleartext
/// `hey-social.follow` feed event for sharing our DM pubkeys + node ticket, so
/// the social graph + routing keys never ride in clear on the public feed topic.
/// Processed by `process_sealed_follows` (background poll); stripped from the
/// visible thread on read (chat_conversation).
const FOLLOW_PREFIX: &str = "\u{1}hey-follow:1:";
/// Reverse of the follow-announce: a sealed control DM telling the followee we UNFOLLOWED them, so
/// they drop us from their followers + rekey (forward secrecy). Without it, unfollow is local-only
/// and the followee shows us as a follower forever + a later re-follow looks like an existing tie.
const UNFOLLOW_PREFIX: &str = "\u{1}hey-unfollow:1:";
/// UI-ONLY block signal: a sealed control DM telling the recipient that the sender BLOCKED them, so
/// their UI can surface "you've been blocked" + disable the composer for this chat. Pure courtesy —
/// the REAL enforcement (dropping their inbound) already lives in is_blocked / is_blocked_follower;
/// this only informs the blocked peer's UI. Recorded in blocked-by-peer.json on receipt.
const BLOCK_PREFIX: &str = "\u{1}hey-block:1:";
/// Reverse of the block signal: the sender UNBLOCKED the recipient → their UI re-enables the composer.
const UNBLOCK_PREFIX: &str = "\u{1}hey-unblock:1:";
/// PRIVATE-FEED key delivery: hidden control message base64(json {author,epoch,key}) over the SAME
/// SOH-prefixed sealed E2E-DM channel. The author hands an ACCEPTED follower the symmetric feed key
/// for the current epoch so they (and only they) can open the author's sealed posts. Bumped + re-sent
/// on follower removal (epoch rotation). The DM sender is the authoritative author (never the payload
/// field). Processed by `process_sealed_follows` (background poll); stripped from the visible thread.
const FEED_KEY_PREFIX: &str = "\u{1}hey-social.feed_key:1:";
/// Receiver→author ACK that a feed key was received for an epoch → the author marks it delivered and
/// stops retrying. Additive + backward-compatible (old peers never send it → author falls back to the
/// attempts/backoff bound, which alone already fixes the stuck case).
const FEED_KEY_ACK_PREFIX: &str = "\u{1}hey-social.feed_key_ack:1:";
/// Hidden 1:1 voice-call control messages (offer / accept / decline / end), base64(json) payload,
/// over the SAME SOH-prefixed E2E-DM channel as the address card — stripped from the visible thread.
/// The native CallManager drives ringing + call state off `call_poll()`.
const CALL_PREFIX: &str = "\u{1}hey-call:1:";
/// Hidden "delete this message" tombstone: base64(json {"id": <msg id>}) over the SAME E2E channel.
/// A reader hides a message only when a tombstone with the SAME `mine`-side references its id — so
/// you can only delete your OWN messages (yours are `mine` to you, and arrive from the same single
/// sender to the peer). Stripped from the visible thread.
const DEL_PREFIX: &str = "\u{1}hey-del:1:";

/// F-TOMBSTONE-AUTHOR: index every NON-control message by its id → its stored
/// VERIFIED `sender_did` (the value `verify_inner` tied to the signature; empty
/// for legacy messages that predate the field). The tombstone collectors consult
/// this so an edit/delete is only honoured when the tombstone's own verified
/// `sender_did` equals the TARGET message's `sender_did` — in a group, member Y
/// can no longer rewrite/blank member X's message. Legacy targets (empty
/// `sender_did`) fall back to the original `(id, mine)` behaviour so existing
/// history isn't broken.
fn target_authors(arr: &[Value]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for m in arr {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        // Skip control rows (del/edit/call) — only real messages are tombstone targets.
        if text.starts_with('\u{1}') {
            continue;
        }
        let Some(id) = m.get("id").and_then(Value::as_str) else { continue };
        if id.is_empty() {
            continue;
        }
        let sd = m.get("sender_did").and_then(Value::as_str).unwrap_or("");
        map.insert(id.to_string(), sd.to_string());
    }
    map
}

/// Author-binding gate shared by both collectors: should a tombstone authored by
/// `tomb_author` (its verified `sender_did`, possibly empty for legacy) be allowed
/// to mutate the target message `id`?
///   • Target has a non-empty `sender_did` (modern): require the tombstone's
///     author to MATCH it. This is the F-TOMBSTONE-AUTHOR fix.
///   • Target unknown OR target `sender_did` empty (legacy history): keep the old
///     behaviour (allow), so pre-field messages stay editable/deletable.
fn tombstone_authorized(
    authors: &std::collections::HashMap<String, String>,
    id: &str,
    tomb_author: &str,
) -> bool {
    match authors.get(id) {
        Some(target_sd) if !target_sd.is_empty() => target_sd == tomb_author,
        _ => true, // legacy (empty sender_did) or not-yet-arrived target → old behaviour
    }
}

/// (id, mine) pairs tombstoned for deletion within a conversation slice. Only
/// tombstones whose verified author matches the target's stored sender_did (or
/// legacy targets) are collected — see `tombstone_authorized`.
fn collect_deleted(arr: &[Value]) -> std::collections::HashSet<(String, bool)> {
    let authors = target_authors(arr);
    let mut set = std::collections::HashSet::new();
    for m in arr {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let Some(b64) = text.strip_prefix(DEL_PREFIX) else { continue };
        let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
        // The tombstone's own VERIFIED author (empty for legacy control rows).
        let tomb_author = m.get("sender_did").and_then(Value::as_str).unwrap_or("");
        if let Ok(bytes) = B64U.decode(b64.trim()) {
            if let Ok(p) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(id) = p.get("id").and_then(Value::as_str) {
                    if tombstone_authorized(&authors, id, tomb_author) {
                        set.insert((id.to_string(), mine));
                    }
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
    let authors = target_authors(arr);
    let mut map = std::collections::HashMap::new();
    for m in arr {
        let text = m.get("text").and_then(Value::as_str).unwrap_or("");
        let Some(b64) = text.strip_prefix(EDIT_PREFIX) else { continue };
        let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
        // The tombstone's own VERIFIED author (empty for legacy control rows).
        let tomb_author = m.get("sender_did").and_then(Value::as_str).unwrap_or("");
        if let Ok(bytes) = B64U.decode(b64.trim()) {
            if let Ok(p) = serde_json::from_slice::<Value>(&bytes) {
                if let (Some(id), Some(new_text)) = (
                    p.get("id").and_then(Value::as_str),
                    p.get("text").and_then(Value::as_str),
                ) {
                    // F-TOMBSTONE-AUTHOR: only the original author may edit.
                    if tombstone_authorized(&authors, id, tomb_author) {
                        map.insert((id.to_string(), mine), new_text.to_string());
                    }
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
    // FOLLOW≠CHAT: a call is a private-communication capability, so it must obey the same isolation
    // as text — a follow-only contact can't be called. The CALL_PREFIX control DM is SOH-prefixed and
    // therefore BYPASSES the send_message_inner gate, so it needs its own check here.
    if !dms::is_chat_enabled(to_did).await {
        return false;
    }
    // F-CALL-UNVERIFIED: mirror share_addresses. A call-control signal to an
    // UNVERIFIED (phishing-link, attacker-substitutable keys) contact would both
    // confirm our online-state and leak our node ticket via the call payload.
    // Fail closed: hold the signal until the user clears the contact (safety-number
    // verify or "send anyway"); a VERIFIED contact rings exactly as before, and a
    // deferred contact can call the instant verification clears this sentinel.
    // Also hold for a key_changed contact (defense-in-depth): a safety-number-changed
    // alarm means re-verify before any sensitive auto-share, even if needs_verify is clear.
    if dms::find_contact(to_did).await.map(|c| c.needs_verify_before_send || c.key_changed).unwrap_or(false) {
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
        // F-BLOCK: a blocked contact must not be able to ring us. Skip their
        // thread entirely so an inbound call signal from them never surfaces.
        if dms::is_blocked(&c.did).await {
            continue;
        }
        // FOLLOW≠CHAT (receive side): a follow-only contact (not chat-enabled) must not be able to
        // ring us — calls obey the same isolation as text, symmetrically.
        if !dms::is_chat_enabled(&c.did).await {
            continue;
        }
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
    // Compact the voice ticket (<=4 direct-IP hints + ALL relays) before it is
    // fanned into the group-call control message to every member, so we never
    // advertise the full uncompacted node ticket. Stays dialable via relay →
    // direct upgrade, exactly like the follow lane.
    let ticket = compact_ticket(&runtime::peer::my_ticket().await.unwrap_or_default());
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
/// `video` rides the "start" announce so a joiner's UI knows it's a video call
/// (surfaced back via `group_call_roster` / `group_active_call` as `"video"`).
pub async fn group_call_start(gid: &str, video: bool) -> Value {
    ensure_session().await.ok();
    let (did, ticket, name) = my_call_identity().await;
    if did.is_empty() {
        return json!({ "ok": false });
    }
    // R6-GCALL-SECRET-SUB (hardened): bind the call_id to the FULL originator DID via a
    // 96-bit collision-resistant tag, not the old 6-char did:key tail (~35 bits, grindable
    // so a co-member could spoof the host / substitute the media secret). Only a start whose
    // verified sender re-derives this exact tag may set the secret + host (see group_call_roster).
    let tag = hey_core::crypto::gcall_origin_tag(&did);
    let call_id = format!("gc-{}-{}", tag, hey_core::plat::now_ms());
    // Group media E2E: mint a per-call secret and ride it in the SEALED "start" announce (post_gcall
    // fans out E2E-sealed pairwise to each member, so only roster members — never a spliced endpoint —
    // receive it). Every member derives the same shared group key from it. `mc:true` advertises that
    // we understand group media keying, so peers only encrypt once EVERYONE is capable.
    let mut secret = hey_core::crypto::random_secret();
    let secret_b64 = B64S.encode(secret);
    secret.fill(0);
    let payload = json!({ "t": "start", "call_id": call_id, "did": did, "ticket": ticket, "name": name, "video": video, "secret": secret_b64, "mc": true });
    let ok = post_gcall(gid, &payload).await;
    json!({ "ok": ok, "call_id": call_id, "ticket": ticket, "video": video })
}

/// Emit a group-call control signal: `join` (entering / heartbeat), `leave` (I left), or `end`
/// (host ended for everyone).
pub async fn group_call_signal(gid: &str, call_id: &str, kind: &str) -> bool {
    if call_id.is_empty() {
        return false;
    }
    let (did, ticket, name) = my_call_identity().await;
    // `mc:true` advertises group-media capability on every join/heartbeat so the roster's
    // all-capable check stays accurate as members come and go.
    let payload = json!({ "t": kind, "call_id": call_id, "did": did, "ticket": ticket, "name": name, "mc": true });
    post_gcall(gid, &payload).await
}

/// Derive the live state of a group call from the group thread: who's present (latest start/join and
/// not since left), each participant's dialable ticket, whether the host ended it, and whether it's
/// still active (recent + non-empty). Drives both the mesh roster and the in-call participant list.
pub async fn group_call_roster(gid: &str, call_id: &str) -> Value {
    ensure_session().await.ok();
    let me = whoami_did().await.unwrap_or_default();
    let now = hey_core::plat::now_ms();
    // Owner-controlled roster: the source of truth for WHO may be in the call and
    // which endpoint each member dials from. A call payload can claim any `did`/
    // `ticket`, so identity comes from the message's VERIFIED `sender_did` and the
    // ticket is accepted only if it resolves to the member's pinned endpoint.
    let group = dms::find_group(gid).await;
    let conv = dms::read_group_conversation(gid).await;
    let arr = serde_json::to_value(&conv)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    // VERIFIED sender_did -> (latest ts, present, ticket-claimed, name)
    let mut state: std::collections::HashMap<String, (i64, bool, String, String)> = std::collections::HashMap::new();
    // VERIFIED did -> media-capable (advertised `mc` on its start/join). Drives whether the call may
    // use app-layer group media encryption (only when EVERY present participant is capable).
    let mut mc_state: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    // The shared group-media secret, read off the authoritative "start" announce only.
    let mut call_secret = String::new();
    // F-GCALL-END-AUTHZ: collect every VERIFIED `end` signal (its sender did) and
    // separately learn who STARTED the call. A call is globally ended ONLY when the
    // host (the verified `start` sender) or the group owner ends it — resolved
    // AFTER the loop because messages aren't guaranteed start-first. A non-host
    // `end` is treated as that member LEAVING, not ending the call for everyone.
    let mut end_dids: Vec<String> = Vec::new();
    let mut host_did = String::new();
    let mut ended = false;
    let mut latest_ts = 0i64;
    // Whether this is a video call — read off the "start" announce so a joiner's
    // UI can open the camera path. (start_ts pins it to THE start, not a later
    // join, in case payloads ever differ.)
    let mut video = false;
    let mut start_ts = -1i64;
    // R6-GCALL-SECRET-SUB (hardened): call_id is "gc-{origin-tag}-{ts}" (group_call_start), where
    // origin-tag = crypto::gcall_origin_tag(originator_did) — a 96-bit HKDF commitment to the FULL
    // originator DID (NOT the old 6-char ~35-bit did:key tail, which a co-member could grind to
    // collide). Only a start from the VERIFIED originator whose did re-derives this exact tag may
    // set the media secret + host; any other member's forged start (even a lower ts) can't match.
    let origin_tag: String = call_id
        .strip_prefix("gc-")
        .and_then(|r| r.rsplit_once('-'))
        .map(|(tag, _)| tag.to_string())
        .unwrap_or_default();
    let mut secret_pinned = false;
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
        // F-GCALL-ROSTER / F-GCALL-SPOOF: derive the participant identity from the
        // message's cryptographically-verified `sender_did` (set by hey-core's
        // verify_inner on receive), NOT from the self-asserted payload `did`. A
        // member therefore can only ever speak for ITSELF — it cannot splice
        // another DID (or an uninvited endpoint) into the mesh. Legacy stored
        // ctrl messages predate sender_did and deserialize to "" — fall back to
        // the payload `did` ONLY for those, never for anything newly received.
        let verified = m.get("sender_did").and_then(Value::as_str).unwrap_or("");
        let did = if verified.is_empty() {
            p.get("did").and_then(Value::as_str).unwrap_or("").to_string()
        } else {
            verified.to_string()
        };
        // The endpoint the payload CLAIMS to dial from — trusted only after the
        // ticket-pinning check below.
        let ticket = p.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
        let name = p.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        latest_ts = latest_ts.max(ts);
        let mc = p.get("mc").and_then(Value::as_bool).unwrap_or(false);
        if kind == "start" {
            // R6-GCALL-SECRET-SUB (hardened): only the call ORIGINATOR — the VERIFIED sender whose
            // did re-derives the 96-bit origin tag embedded in call_id — may set the media secret +
            // host. Pin on the FIRST such start; ignore every other start. A forged start from
            // another member (even with a lower ts, which the prior min-ts pin was fooled by) can't
            // produce a matching tag without a ~2^96 second-preimage on their own DID. `verified`
            // is the crypto-verified sender_did (empty on legacy ctrl) — never the spoofable payload.
            let sender_tag = hey_core::crypto::gcall_origin_tag(verified);
            let from_origin =
                !verified.is_empty() && !origin_tag.is_empty() && sender_tag == origin_tag;
            if from_origin && !secret_pinned {
                secret_pinned = true;
                video = p.get("video").and_then(Value::as_bool).unwrap_or(false);
                if let Some(s) = p.get("secret").and_then(Value::as_str) {
                    call_secret = s.to_string();
                }
                host_did = did.clone(); // the verified originator IS the host
            } else if !from_origin {
                if let Some(s) = p.get("secret").and_then(Value::as_str) {
                    if !s.is_empty() && s != call_secret {
                        log::warn!("group call: ignoring a start from a non-originator (secret-substitution attempt blocked)");
                    }
                }
            }
            // Keep the latest-start tracker (call-liveness) independent of the secret pin.
            if ts >= start_ts {
                start_ts = ts;
            }
        }
        if kind == "end" && !did.is_empty() {
            // Authorization is decided after the loop (host/owner only). Record the
            // verified sender so we can resolve it once host_did is known.
            end_dids.push(did.clone());
        }
        if did.is_empty() {
            continue;
        }
        if matches!(kind, "start" | "join") {
            mc_state.insert(did.clone(), mc);
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
                // A non-host `end` is a LEAVE for the sender (presence false); a
                // host/owner `end` additionally ends the whole call (resolved below).
                "leave" | "end" => e.1 = false,
                _ => {}
            }
        }
    }
    // F-GCALL-END-AUTHZ: end the call for everyone ONLY if an `end` came from the
    // host (verified start sender) OR the group owner. Any other member's `end`
    // already dropped THAT member's presence above (LEAVE), but never ends the call.
    let owner = group.as_ref().map(|g| g.created_by.clone()).unwrap_or_default();
    for d in &end_dids {
        if (!host_did.is_empty() && *d == host_did) || (!owner.is_empty() && *d == owner) {
            ended = true;
            break;
        }
    }
    // Build the authorized roster: each present member must be a CURRENT, non-barred
    // member of the owner-controlled group, and the ticket we hand the mesh must
    // resolve to that member's pinned endpoint (never an attacker-supplied one).
    let mut participants: Vec<Value> = Vec::new();
    for (did, v) in state.iter() {
        if !v.1 {
            continue; // not present (left)
        }
        let mine = *did == me;
        if !mine {
            // F-GCALL-ROSTER: read-time membership + kick/block filter. A DID not
            // in the live roster (or one kicked mid-call) is dropped, even if it
            // self-announced a join. When NO group has materialised yet (legacy /
            // same-runtime pre-roster call) we keep the prior behaviour so existing
            // calls still work; an existing group that omits the DID rejects it.
            match group.as_ref() {
                Some(g) if dms::group_member_authorized(g, did) => {}
                None => {}
                _ => continue,
            }
        }
        // Ticket pinning: trust the payload ticket ONLY when its decoded EndpointId
        // matches the endpoint we already know for this member (the owner-vouched
        // roster ticket, or our pinned contact ticket). Otherwise dial the PINNED
        // ticket — never the claimed one. My own tile keeps my own live ticket.
        let claimed = v.2.clone();
        let pinned = if mine {
            String::new()
        } else {
            // F-OWNER-TICKET-PoP: the dial endpoint comes ONLY from the member's OWN
            // self-asserted ticket (a ticket carried in a message we verified came
            // from THEM). The owner-set roster ticket (group_member_peer_ticket) and
            // the owner-poisonable contact ticket (peer_ticket) are NO LONGER used to
            // pick the dial anchor — a malicious owner can set members[M].peer_ticket
            // = Eve (or poison M's contact ticket) and redirect M's media stream to a
            // non-member. Membership/identity still come from group_member_authorized
            // + the verified sender_did above; only the DIAL ENDPOINT is restricted to
            // M's own self-assertion. Empty ⇒ fail closed below (F-GCALL-PIN-EMPTY-SPLICE).
            dms::peer_ticket_self_asserted(did).await.unwrap_or_default()
        };
        let ticket = if mine {
            // My own tile: keep my own live (claimed) ticket. Identity is the
            // verified did.
            claimed
        } else if pinned.is_empty() {
            // F-GCALL-PIN-EMPTY-SPLICE: no pinned endpoint for this member (no
            // owner-vouched roster ticket AND no pinned contact ticket). Fall
            // CLOSED — drop the participant rather than dial a SELF-CLAIMED
            // ticket. The old `claimed` fallback let a member splice a
            // non-member's EndpointId here and stream the whole call to it. The
            // member's identity still resolves to the verified did; only their
            // dialable endpoint is withheld until the owner vouches one (the
            // roster re-derives every poll, so the tile reappears the moment a
            // pinned/vouched ticket exists).
            continue;
        } else if claimed.is_empty() || endpoints_match(&claimed, &pinned).await {
            // Claimed ticket resolves to the member's KNOWN endpoint → accept it
            // (it may carry fresher relay/socket hints). Empty claim → use pinned.
            if claimed.is_empty() { pinned } else { claimed }
        } else {
            // Claimed ticket resolves to a DIFFERENT endpoint than the one tied to
            // this member → REJECT the splice; dial the pinned endpoint instead.
            pinned
        };
        participants.push(json!({ "did": did, "ticket": ticket, "name": v.3, "mine": mine }));
    }
    let stale = now - latest_ts > 120_000;
    let active = !ended && !stale && !participants.is_empty();
    // Group media encryption may activate ONLY when EVERY present participant advertised `mc` (and we
    // hold the secret). Self counts as capable (this build). Any not-yet-updated member ⇒ false ⇒ the
    // app keeps the call plaintext so nobody is fed un-decryptable noise.
    let all_media_capable = !call_secret.is_empty()
        && participants.iter().all(|p| {
            let d = p.get("did").and_then(Value::as_str).unwrap_or("");
            p.get("mine").and_then(Value::as_bool).unwrap_or(false) || *mc_state.get(d).unwrap_or(&false)
        });
    json!({ "active": active, "ended": ended, "call_id": call_id, "video": video, "participants": participants, "secret": call_secret, "all_media_capable": all_media_capable })
}

/// True iff two carrier tickets/bare-ids decode to the SAME iroh EndpointId —
/// the identity check that pins a group-call payload ticket to a member's known
/// endpoint (F-GCALL-ROSTER). Endpoint-only comparison (relay/socket hints are
/// ignored), so a fresher ticket for the same node still matches. Returns false
/// when the carrier isn't up or either side is undecodable (fail closed).
async fn endpoints_match(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true; // identical encoding — same endpoint, no carrier needed
    }
    if let Some((_h, slot)) = crate::NET.get() {
        if let Some(c) = slot.read().await.clone() {
            if let (Some(ea), Some(eb)) = (c.peer_id_of(a), c.peer_id_of(b)) {
                return ea == eb;
            }
        }
    }
    false
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
        None => json!({ "active": false, "ended": false, "video": false, "participants": [] }),
    }
}

/// Send my published tip addresses to a contact over the E2E DM channel, ONCE per
/// peer. Idempotent on the receiver (cache merge). Returns false if I have no
/// addresses yet (provision first) or there's no DM channel to them.
pub async fn share_addresses(to_did: &str) -> bool {
    if to_did.is_empty() {
        return false;
    }
    // F-ADDR-CARD-UNVERIFIED: this auto-fires on chat-open and seals my wallet
    // {chain:address} map via the SOH `hey-addr:` card, which is EXEMPT from the
    // needs_verify_before_send send gate (dms.rs:3514 skips SOH-prefixed control
    // messages). Without this check a phishing-link contact (keys unverified,
    // attacker-substitutable) would receive my addresses the instant a chat opens.
    // Fail closed: hold the card until the user clears the contact (safety-number
    // verify or "send anyway"). chat_conversation/refresh_contact_addresses call
    // this on every open, so the addresses RE-SHARE automatically once verified.
    // Hold the wallet card for a verify-gated OR key_changed contact (defense-in-depth): a changed
    // safety number means re-verify before re-sharing the real {chain:address} map.
    if dms::find_contact(to_did).await.map(|c| c.needs_verify_before_send || c.key_changed).unwrap_or(false) {
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

/// Seed a peer's nickname from a post's denormalized `author_name` into their
/// profile cache, so CHAT shows the real name too — not just the feed. Only fills
/// a MISSING nickname; a signed profile broadcast (which carries a `ts`) stays
/// authoritative and is never clobbered.
async fn seed_peer_nickname(did: &str, name: &str) {
    if did.is_empty() || name.is_empty() {
        return;
    }
    let _g = storage_lock().lock().await;
    let key = format!("hey-social/peers/{did}.json");
    let mut p = shared_read_json(&key).await.ok().flatten().unwrap_or_else(|| json!({}));
    if !p.is_object() {
        p = json!({});
    }
    if p.get("nickname").and_then(Value::as_str).map(|s| !s.is_empty()).unwrap_or(false) {
        return; // already have a (likely broadcast) nickname — don't overwrite it
    }
    p["nickname"] = json!(name);
    let _ = shared_write_json(&key, &p).await;
}

/// Best-effort display name for a peer DID — their cached (broadcast) nickname, else a short DID.
/// Used for like/comment notifications where the wire event carries no name.
async fn peer_display_name(did: &str) -> String {
    shared_read_json(&format!("hey-social/peers/{did}.json"))
        .await
        .ok()
        .flatten()
        .and_then(|p| p.get("nickname").and_then(Value::as_str).map(String::from))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| did.trim_start_matches("did:key:z").chars().take(10).collect())
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
    let cached_nick = cache[&author]
        .get("nickname")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let cached_avatar = cache[&author]
        .get("avatar")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    if let Some(n) = cached_nick {
        p["author_name"] = json!(n);
    } else if let Some(an) = p
        .get("author_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
    {
        // No cached profile nickname yet, but the post carries the author's name.
        // Persist it so CHAT (chat_contacts' overlay) shows the real nickname too,
        // not only the feed. Memoize so we don't rewrite for later posts this build.
        seed_peer_nickname(&author, &an).await;
        if let Some(obj) = cache.get_mut(&author).and_then(|v| v.as_object_mut()) {
            obj.insert("nickname".to_string(), json!(an));
        }
    }
    if let Some(a) = cached_avatar {
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
    // Bump the monotonic revision so followers' ingest freshness gate accepts THIS edit but rejects
    // a later replay of the older event.
    post["edited_ts"] = json!(hey_core::plat::now_ms());
    shared_write_json(&format!("hey-social/posts/{id}.json"), &post).await.map_err(|e| e.to_string())?;
    // PRIVATE FEED: re-broadcast SEALED under my current epoch (see create_post).
    let wire = seal_post_outbound(&post).await;
    publish(&feed_topic(&me), "hey-social.post", wire).await;
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
    // TOMBSTONE (not Null): a Null scores rev=0 in the ingest freshness gate, so a replayed
    // original/edit event (rev>0) would pass and RESURRECT a deleted post. Store a tombstone
    // carrying a high monotonic rev (delete time) so every replay of the (older) post is rejected
    // and it stays deleted. The feed reads via the index (tombstone removed → never rendered);
    // get_post returns no author so react/comment on it is refused.
    let _ = shared_write_json(
        &format!("hey-social/posts/{id}.json"),
        &json!({ "id": id, "deleted": true, "edited_ts": hey_core::plat::now_ms() }),
    )
    .await;
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
            // PRIVATE FEED: seal the emoji under the post author's feed key (op + post_id stay
            // cleartext for routing). Skip the broadcast if I somehow hold no key (no cleartext leak).
            if let Some((epoch, sealed)) = seal_for_feed(author, &json!({ "emoji": emoji })).await {
                publish(
                    &feed_topic(author),
                    "hey-social.react",
                    json!({ "post_id": post_id, "op": op, "epoch": epoch, "sealed": sealed, "enc": true }),
                )
                .await;
            }
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
        if list.len() > MAX_COMMENTS_PER_POST {
            let drop_n = list.len() - MAX_COMMENTS_PER_POST;
            list.drain(0..drop_n);
        }
        shared_write_json(&key, &json!(list)).await.map_err(|e| e.to_string())?;
    }
    // Broadcast on the post author's topic. PRIVATE FEED: seal the comment CONTENT
    // (author_name + text) under the post author's feed key; keep routing (id/author/ts/parent)
    // cleartext so the receiver can validate signer==author, dedup, and thread without the key.
    if let Ok(p) = get_post(post_id).await {
        if let Some(author) = p.get("author").and_then(Value::as_str) {
            let secret = json!({
                "author_name": c.get("author_name").cloned().unwrap_or_default(),
                "text": c.get("text").cloned().unwrap_or_default(),
            });
            if let Some((epoch, sealed)) = seal_for_feed(author, &secret).await {
                let cwire = json!({
                    "id": c.get("id").cloned().unwrap_or(Value::Null),
                    "author": me,
                    "ts": c.get("ts").cloned().unwrap_or(Value::Null),
                    "parent": c.get("parent").cloned().unwrap_or(Value::Null),
                    "epoch": epoch,
                    "sealed": sealed,
                    "enc": true,
                });
                publish(
                    &feed_topic(author),
                    "hey-social.comment",
                    json!({ "post_id": post_id, "comment": cwire }),
                )
                .await;
            }
        }
    }
    Ok(c)
}

pub async fn get_comments(post_id: &str) -> Result<Value, String> {
    let key = format!("hey-social/comments/{post_id}.json");
    Ok(shared_read_json(&key).await.ok().flatten().unwrap_or_else(|| json!([])))
}

// ── follow / social graph ────────────────────────────────────────────────────

/// Canonical bytes signed/verified for a friend link's PQ chat keys: a
/// FIXED-key-order JSON object over `{did,k,ticket,x}`. Both the signer
/// (`my_friend_link`) and the verifier (`parse_follow`) build it identically so
/// the Ed25519 signature round-trips byte-for-byte. Empty keys (no PQ pubkeys)
/// canonicalize to "" so a keyless link never produces a spurious signature.
fn canonical_follow_msg(did: &str, x: &str, k: &str, ticket: &str) -> Vec<u8> {
    json!({ "did": did, "k": k, "ticket": ticket, "x": x })
        .to_string()
        .into_bytes()
}

/// A shareable follow link: `hey:follow:<base64url(json{did,ticket,x,k,sig})>`.
/// Carries our DM pubkeys (x=X25519, k=ML-KEM) so following also makes us
/// chat-able — the other side can "Message" us with no extra handshake.
/// `sig` = our Ed25519 signature (hex) over canonical({did,x,k,ticket}), proof we
/// own the advertised did:key so an in-path attacker can't substitute the PQ keys.
pub async fn my_friend_link() -> Result<String, String> {
    let w = whoami().await?;
    let did = w.get("did").and_then(Value::as_str).unwrap_or("").to_string();
    let pk = dms::my_pubkeys().await;
    // Trim direct-IP addrs from the ticket so the link/QR stays small + scannable;
    // the peer connects via the relay, then iroh upgrades to a direct path anyway.
    let ticket = compact_ticket(w.get("ticket").and_then(Value::as_str).unwrap_or(""));
    let x = pk.as_ref().map(|k| k.x25519_pub_b64.clone()).unwrap_or_default();
    let k = pk.as_ref().map(|k| k.ml_kem_pub_b64.clone()).unwrap_or_default();
    let mut payload = json!({
        "did": did,
        "ticket": ticket,
        "x": pk.as_ref().map(|k| k.x25519_pub_b64.clone()),
        "k": pk.as_ref().map(|k| k.ml_kem_pub_b64.clone()),
    });
    // Sign only when we actually advertise PQ keys (the thing worth protecting);
    // a keyless link stays feed-only/unsigned. Best-effort: if the session seed
    // is unavailable we still emit the (unsigned) link — recipients fall back to
    // the unverified-pin path, exactly as before this change.
    if !x.is_empty() && !k.is_empty() {
        // Sign the link with our DID Ed25519 key so the scanner can pin our PQ keys as
        // VERIFIED (keys_verified=true) and the anti-phishing follow-announce gate can stay
        // closed for genuinely-forged/unsigned links. Prefer the session seed; on provider-
        // backed MOBILE sessions auth_key_hex is empty, so fall back to the runtime-held
        // IDENTITY seed (the same key verse_gossip signs presence with), behind the
        // identity/sign capability gate. This is what makes mobile QR pairing produce SIGNED
        // links — the fix for "every mobile link is unsigned" that broke the keys_verified gate.
        let seed: Option<[u8; 32]> = session::current()
            .and_then(|s| hex_seed32(&s.auth_key_hex).ok())
            .or_else(|| {
                if crate::guard::check("identity", "sign").is_ok() {
                    crate::IDENTITY.get().map(|id| id.seed())
                } else {
                    None
                }
            });
        if let Some(seed) = seed {
            let sig = hey_core::identity::sign(&canonical_follow_msg(&did, &x, &k, &ticket), &seed);
            payload["sig"] = json!(sig);
        }
    }
    Ok(format!("hey:follow:{}", B64U.encode(payload.to_string())))
}

/// Decode a 32-byte Ed25519 seed from hex (the session `auth_key_hex`).
fn hex_seed32(hex: &str) -> Result<[u8; 32], String> {
    let v = hey_core::identity::hex_to_bytes(hex)?;
    if v.len() != 32 {
        return Err("seed must be 32 bytes".into());
    }
    let mut s = [0u8; 32];
    s.copy_from_slice(&v);
    Ok(s)
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

/// What a scanned/pasted link is FOR. The intent lives in the link itself (the `hyper:` scheme +
/// a domain-separated magic byte + signature), so a follow link can NEVER open a chat and a chat
/// link can NEVER follow — enforced in follow_impl. Legacy `hey:follow:` carries no intent, so the
/// caller's chat_only flag decides (unchanged behavior).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LinkIntent {
    Follow,
    Chat,
    Legacy,
}

// ── Binary "hyper:" link container — same data + security as the legacy JSON link, ~30% smaller QR.
// The density win is purely encoding: the big keys ride as RAW bytes (no base64-inside-JSON-inside-
// base32 double-encoding). Layout:
//   [0]              magic: 0xF1 = follow, 0xC1 = chat  (domain separation: a follow sig can't
//                    validate as a chat link and vice-versa, because magic is the first signed byte)
//   [1..33]          did Ed25519 pubkey (32)            -> reconstruct did:key on decode
//   [33..65]         x = X25519 pub (32)
//   [65..1249]       k = ML-KEM-768 pub (1184)          -> PQ kept, in full, self-contained (no OOB)
//   [1249..1249+E]   extra (chat only: q‖r‖no‖exp; empty for follow)
//   [..]             ticket tag(1: 0=base32-of-raw,1=utf8) ‖ len(u16 BE) ‖ ticket bytes
//   [..+64]          sig = Ed25519 over EVERYTHING before sig (anti-substitution: binds did->keys)
const HYPER_MAGIC_FOLLOW: u8 = 0xF1;
const HYPER_MAGIC_CHAT: u8 = 0xC1;
const ML_KEM_PUB_LEN: usize = 1184;
const HYPER_HEAD: usize = 1 + 32 + 32 + ML_KEM_PUB_LEN; // magic + did + x + k = 1249

fn ticket_to_raw(ticket: &str) -> (u8, Vec<u8>) {
    // Prefer the ticket's RAW bytes (base32-decoded) for density; fall back to utf8 text if the
    // ticket isn't clean base32 (the compact_ticket fallback-to-input case).
    match data_encoding::BASE32_NOPAD.decode(ticket.as_bytes()) {
        Ok(raw) => (0, raw),
        Err(_) => (1, ticket.as_bytes().to_vec()),
    }
}
fn ticket_from_raw(tag: u8, bytes: &[u8]) -> String {
    if tag == 0 {
        data_encoding::BASE32_NOPAD.encode(bytes)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Build a signed binary `hyper:` link body, base64url-wrapped. `extra` is the chat-only field block
/// (empty for follow). None if keys/seed are missing/malformed.
fn encode_hyper_link(
    magic: u8,
    scheme: &str,
    did: &str,
    x_b64: &str,
    k_b64: &str,
    ticket: &str,
    extra: &[u8],
    seed: &[u8; 32],
) -> Option<String> {
    let did_pk = hey_core::identity::did_key_to_public_key(did).ok()?;
    let x = B64S.decode(x_b64).ok()?;
    let k = B64S.decode(k_b64).ok()?;
    if x.len() != 32 || k.len() != ML_KEM_PUB_LEN {
        return None;
    }
    let (ttag, tbytes) = ticket_to_raw(ticket);
    if tbytes.len() > u16::MAX as usize {
        return None;
    }
    let mut c = Vec::with_capacity(HYPER_HEAD + extra.len() + 3 + tbytes.len() + 64);
    c.push(magic);
    c.extend_from_slice(&did_pk);
    c.extend_from_slice(&x);
    c.extend_from_slice(&k);
    c.extend_from_slice(extra);
    c.push(ttag);
    c.extend_from_slice(&(tbytes.len() as u16).to_be_bytes());
    c.extend_from_slice(&tbytes);
    let sig = hey_core::identity::hex_to_bytes(&hey_core::identity::sign(&c, seed)).ok()?;
    if sig.len() != 64 {
        return None;
    }
    c.extend_from_slice(&sig);
    Some(format!("{scheme}{}", B64U.encode(&c)))
}

/// Decode + VERIFY a binary `hyper:` link body. Returns (did, ticket, x_b64, k_b64, extra, verified).
/// hyper: links are ALWAYS signed — an invalid/absent signature returns None (REJECT), so a tampered
/// key can never be pinned. `extra_len` is the chat field-block size (0 for follow).
fn decode_hyper_link(
    body: &str,
    expect_magic: u8,
    extra_len: usize,
) -> Option<(String, String, String, String, Vec<u8>, bool)> {
    let c = B64U.decode(body).ok()?;
    if c.len() < HYPER_HEAD + extra_len + 3 + 64 || c[0] != expect_magic {
        return None;
    }
    let did_pk: [u8; 32] = c[1..33].try_into().ok()?;
    let did = hey_core::identity::public_key_to_did_key(&did_pk);
    let x_b64 = B64S.encode(&c[33..65]);
    let k_b64 = B64S.encode(&c[65..HYPER_HEAD]);
    let mut off = HYPER_HEAD;
    let extra = c[off..off + extra_len].to_vec();
    off += extra_len;
    let ttag = c[off];
    off += 1;
    let tlen = u16::from_be_bytes([c[off], c[off + 1]]) as usize;
    off += 2;
    if c.len() != off + tlen + 64 {
        return None; // exact length — no trailing garbage, no short read
    }
    let ticket = ticket_from_raw(ttag, &c[off..off + tlen]);
    off += tlen;
    let canonical = &c[..off];
    let sig = &c[off..off + 64];
    let pk = hey_core::identity::did_key_to_public_key(&did).ok()?;
    if !hey_core::identity::verify(canonical, &hey_core::identity::bytes_to_hex(sig), &pk) {
        return None;
    }
    Some((did, ticket, x_b64, k_b64, extra, true))
}

/// Resolve the signing seed (session seed, else the runtime IDENTITY seed behind the sign gate) —
/// the same precedence `my_friend_link` uses. None if neither is available.
fn link_signing_seed() -> Option<[u8; 32]> {
    session::current()
        .and_then(|s| hex_seed32(&s.auth_key_hex).ok())
        .or_else(|| {
            if crate::guard::check("identity", "sign").is_ok() {
                crate::IDENTITY.get().map(|id| id.seed())
            } else {
                None
            }
        })
}

/// A SLIM, FOLLOW-ONLY shareable link: `hyper:follow:<base64url(binary)>`. Same payload as the legacy
/// `hey:follow:` friend link (did + ticket + X25519 + ML-KEM-768 + Ed25519 sig) — full post-quantum
/// keys, self-contained (no OOB), invite-only, offline-pairable — just binary-packed so the QR is
/// ~30% smaller. Distinct scheme ⇒ scanning it can ONLY follow, never chat.
pub async fn my_follow_link() -> Result<String, String> {
    let w = whoami().await?;
    let did = w.get("did").and_then(Value::as_str).unwrap_or("").to_string();
    let pk = dms::my_pubkeys().await;
    let ticket = compact_ticket(w.get("ticket").and_then(Value::as_str).unwrap_or(""));
    let x = pk.as_ref().map(|k| k.x25519_pub_b64.clone()).unwrap_or_default();
    let k = pk.as_ref().map(|k| k.ml_kem_pub_b64.clone()).unwrap_or_default();
    if x.is_empty() || k.is_empty() {
        return Err("no PQ keys yet (not signed in)".into());
    }
    let seed = link_signing_seed().ok_or("no signing seed")?;
    encode_hyper_link(HYPER_MAGIC_FOLLOW, "hyper:follow:", &did, &x, &k, &ticket, &[], &seed)
        .ok_or_else(|| "failed to build follow link".into())
}

/// A SLIM, CHAT-ONLY shareable link: `hyper:chat:<base64url(binary)>`. Same payload + binary packing
/// as hyper:follow (full PQ keys, self-contained, ~30% smaller QR), but a distinct scheme + signed
/// magic byte (0xC1) so scanning it can ONLY start a private chat — never follow. Consumed by
/// chat_from_link (pairs a 1:1 chat from the carried keys, no feed relationship), matching the
/// existing chat-tab QR behavior, just slimmer and intent-explicit.
pub async fn my_chat_link() -> Result<String, String> {
    let w = whoami().await?;
    let did = w.get("did").and_then(Value::as_str).unwrap_or("").to_string();
    let pk = dms::my_pubkeys().await;
    let ticket = compact_ticket(w.get("ticket").and_then(Value::as_str).unwrap_or(""));
    let x = pk.as_ref().map(|k| k.x25519_pub_b64.clone()).unwrap_or_default();
    let k = pk.as_ref().map(|k| k.ml_kem_pub_b64.clone()).unwrap_or_default();
    if x.is_empty() || k.is_empty() {
        return Err("no PQ keys yet (not signed in)".into());
    }
    let seed = link_signing_seed().ok_or("no signing seed")?;
    encode_hyper_link(HYPER_MAGIC_CHAT, "hyper:chat:", &did, &x, &k, &ticket, &[], &seed)
        .ok_or_else(|| "failed to build chat link".into())
}

/// Accept either a raw `did:key:z…` or a `hey:follow:…` link.
/// Returns (did, ticket, Option<(x25519_pub_b64, ml_kem_pub_b64)>, keys_verified).
///
/// `keys_verified` is the trust to pin the PQ keys with (mirrors
/// `decode_invite_link`'s proven gate):
///   • signed link, sig verifies against the DID's Ed25519 key → TRUE
///     (so the existing pin-mismatch refusal then protects against substitution)
///   • signed link, sig INVALID → returns `None` (REJECT — forged/tampered)
///   • UNSIGNED link with keys (old distributed links) → FALSE
///     (still works, but pinned UNVERIFIED so the UI can flag it — backward-compat)
///   • keyless `did:key:` → no keys, FALSE (feed-only; unchanged)
fn parse_follow(input: &str) -> Option<(String, String, Option<(String, String)>, bool, LinkIntent)> {
    let s = input.trim();
    // NEW slim binary scheme — the intent is in the scheme AND the signed magic byte, so a follow
    // link can never be consumed as a chat (decode rejects a wrong magic / invalid signature).
    if let Some(rest) = s.strip_prefix("hyper:follow:") {
        let (did, ticket, x, k, _extra, verified) =
            decode_hyper_link(rest, HYPER_MAGIC_FOLLOW, 0)?;
        return Some((did, ticket, Some((x, k)), verified, LinkIntent::Follow));
    }
    if let Some(rest) = s.strip_prefix("hyper:chat:") {
        let (did, ticket, x, k, _extra, verified) =
            decode_hyper_link(rest, HYPER_MAGIC_CHAT, 0)?;
        return Some((did, ticket, Some((x, k)), verified, LinkIntent::Chat));
    }
    if let Some(rest) = s.strip_prefix("hey:follow:") {
        if let Ok(bytes) = B64U.decode(rest) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                let did = v.get("did").and_then(Value::as_str)?.to_string();
                let ticket = v.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
                let keys = match (v.get("x").and_then(Value::as_str), v.get("k").and_then(Value::as_str)) {
                    (Some(x), Some(k)) => Some((x.to_string(), k.to_string())),
                    _ => None,
                };
                // Verify the proof-of-possession over the advertised PQ keys.
                let mut verified = false;
                if let Some((x, k)) = keys.as_ref() {
                    match v.get("sig").and_then(Value::as_str) {
                        Some(sig) => {
                            // Signed: verify against the pubkey embedded in `did`.
                            // Valid → pin verified; present-but-invalid → REJECT.
                            let pk = hey_core::identity::did_key_to_public_key(&did).ok()?;
                            let msg = canonical_follow_msg(&did, x, k, &ticket);
                            if hey_core::identity::verify(&msg, sig, &pk) {
                                verified = true;
                            } else {
                                return None;
                            }
                        }
                        // Unsigned legacy link with keys → backward-compat: keep
                        // working but pin UNVERIFIED (verified stays false).
                        None => {}
                    }
                }
                return Some((did, ticket, keys, verified, LinkIntent::Legacy));
            }
        }
    }
    // A bare did:key carries NO node ticket (no routing) and NO PQ keys, so it can neither deliver a
    // follow request nor open a sealed channel — this was the removed follow_by_did path. Reject it
    // here so the keyless, un-deliverable follow can't be reached even via the raw FFI (the mobile UI
    // already blocks bare DIDs). Following/messaging require a key-bearing hey:follow friend link.
    None
}

/// Decode a follow/chat link for a confirmation UI (deep-link / paste) WITHOUT acting on it. Returns
/// JSON `{kind,did,verified,has_keys}` — `kind` is "follow"|"chat" (intent-bound for hyper: links,
/// "follow" for legacy hey:follow:). Returns `{}` for anything it can't decode (e.g. hey-invite:,
/// which the Kotlin JSON previewer still handles). Binary hyper: links are parsed here so Kotlin
/// never has to replicate the binary layout.
pub fn preview_link(link: &str) -> String {
    if let Some((did, _t, keys, verified, intent)) = parse_follow(link) {
        let kind = match intent {
            LinkIntent::Chat => "chat",
            _ => "follow",
        };
        return json!({ "kind": kind, "did": did, "verified": verified, "has_keys": keys.is_some() })
            .to_string();
    }
    "{}".to_string()
}

/// Bootstrap a DM-capable contact from a peer's advertised pubkeys (+ticket).
/// `name` is any known nickname (invite/follow payload/post author) so a fresh
/// contact is never created with an empty `DmContact.name` (which would force the
/// Kotlin shortDid fallback); pass "" when truly unknown.
async fn bootstrap_dm(did: &str, name: &str, keys: &Option<(String, String)>, ticket: &str) {
    if let Some((x, k)) = keys {
        // MAX_AUTO_CONTACTS ceiling: re-bootstrapping an EXISTING contact is always
        // allowed (idempotent reconcile — the re-pair self-heal must keep working),
        // but refuse to CREATE a brand-new auto-bootstrapped contact past the cap so
        // a follow flood can't grow the ratchet/contact set without bound.
        if dms::find_contact(did).await.is_none() {
            if dms::list_contacts().await.len() >= MAX_AUTO_CONTACTS {
                log::warn!("contacts at MAX_AUTO_CONTACTS ({MAX_AUTO_CONTACTS}); skip auto-bootstrap");
                crate::guard::audit("contact.cap", json!({ "did": did, "cap": MAX_AUTO_CONTACTS }));
                return;
            }
        }
        let _ = dms::bootstrap_contact_from_keys(
            did,
            name,
            dms::PeerKeys { x25519_pub_b64: x.clone(), ml_kem_pub_b64: k.clone() },
            if ticket.is_empty() { None } else { Some(ticket.to_string()) },
            false,
        )
        .await;
    }
}

pub async fn follow(input: &str) -> Result<Value, String> {
    follow_impl(input, false).await
}

/// CHAT-ONLY connect from a scanned/pasted friend link: pairs a 1:1 chat (bidirectional) WITHOUT
/// following — no following.json entry, no persistent feed subscription/backfill, and the peer is
/// NOT recorded as a follower or shown "started following you" (the key bundles ride with
/// chat_only=true, so both receive paths bootstrap a chat contact only). This is what isolates a
/// scanned chat QR from the feed.
pub async fn chat_from_link(input: &str) -> Result<Value, String> {
    follow_impl(input, true).await
}

async fn follow_impl(input: &str, chat_only: bool) -> Result<Value, String> {
    let (did, ticket, keys, keys_verified, intent) =
        parse_follow(input).ok_or("not a valid DID or friend link")?;
    // DISTINCT-SCHEME ISOLATION: a hyper:follow link can ONLY follow; a hyper:chat link can ONLY
    // chat. The intent is cryptographically bound (signed magic byte), so this can't be spoofed by
    // pasting a follow link into the chat box or vice-versa. Legacy hey:follow: has no intent → the
    // caller's chat_only flag decides (unchanged behavior).
    match intent {
        LinkIntent::Follow if chat_only => {
            return err("that's a follow link — open it from Follow, not New chat");
        }
        LinkIntent::Chat if !chat_only => {
            return err("that's a chat link — open it from New chat, not Follow");
        }
        _ => {}
    }
    let me = whoami_did().await.unwrap_or_default();
    if did == me {
        return err("that's your own DID");
    }
    // Explicit re-add → clear any delete-chat tombstone so the reconcile can rebuild the contact.
    // CHAT-only (Message someone / re-scan a chat link) un-hides so the chat re-pairs and shows.
    // FOLLOW is a FEED action: it must NOT surface an empty chat row — hide until a real message
    // (an active chat with history, last_ts>0, is left visible; only a fresh/empty one is hidden).
    if chat_only {
        dms::unhide_chat(&did).await;
        // CHAT-CAPABILITY: scanning a CHAT link is the explicit consent that permits a private chat.
        // (A FOLLOW never calls this — so following alone can never enable chat.)
        dms::enable_chat(&did).await;
    } else {
        dms::hide_chat_if_empty(&did).await;
    }
    // RE-PAIR FIX: deliberately following/scanning someone is an explicit "connect" — so clear any
    // stale BLOCK on them (from a prior remove_follower). Without this, after A removes+blocks B,
    // a later re-scan of B's QR is silently dropped by is_blocked_follower at record_follower, so
    // the chat never re-pairs and messages never arrive. (Blocking still works: it requires THIS
    // device's deliberate re-add to lift — a remote peer can't unblock itself.)
    let _ = unblock_follower(&did).await;
    // FOLLOW only: record them in following.json (a feed relationship). A CHAT-ONLY connect skips
    // this entirely — it pairs a chat without ever following.
    if !chat_only {
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
                // FOLLOW-REQUEST: a new follow is PENDING until the author accepts and hands us
                // their feed key. flip_following_pending_off() clears this when the key arrives, so
                // the UI shows "Requested" until then (never a false "Following" with no posts).
                "pending": true,
            }));
            shared_write_json("hey-social/following.json", &json!(list)).await.ok();
        }
    }
    // Pin the PQ chat keys with the trust the link earned: a verified signature
    // pins key_verified=TRUE (so a later substitution is refused by the pin-
    // mismatch guard); an unsigned legacy link pins UNVERIFIED so delivery keeps
    // working but the contact is flagged. Mirrors bootstrap_dm but threads the flag.
    if let Some((x, k)) = keys.as_ref() {
        let _ = dms::bootstrap_contact_from_keys(
            &did,
            "",
            dms::PeerKeys { x25519_pub_b64: x.clone(), ml_kem_pub_b64: k.clone() },
            if ticket.is_empty() { None } else { Some(ticket.clone()) },
            keys_verified,
        )
        .await;
        // Fix 2: idempotent self-repair — back-fill any missing per-pair
        // queues/pseudonyms so a re-followed (or partially torn-down) contact is
        // is_v2_active() again. Runs BEFORE the verify-gate set below; it only heals
        // a FALSE gate on an already-verified+unchanged contact, so it never
        // un-gates a genuinely unverified key (mark_needs_verify_before_send still
        // (re)sets the gate for unsigned links right after). lift_hidden = chat_only:
        // a deliberate CHAT scan un-hides a deleted chat; a FOLLOW must NOT (else it
        // un-does the hide_chat_if_empty above and pops an empty feed-only chat row).
        dms::repair_contact(&did, chat_only).await;
        // F-FOLLOW-PoP: an UNSIGNED key-bearing follow link carries no
        // proof-of-possession over its PQ keys (anyone could have minted it with
        // substituted keys). Gate the FIRST seal to such a contact on explicit
        // user confirmation. mark_needs_verify_before_send is a no-op for an
        // already-verified contact or one we've already messaged (grandfathered),
        // so re-following an established contact never blocks. Signed links pin
        // verified and skip this entirely.
        if !keys_verified {
            dms::mark_needs_verify_before_send(&did, true).await;
        }
        // Re-pair robustness: lift any leave-tombstone on this contact's queues so a chat that
        // was previously DELETED re-meshes (gossip_join clears the tombstone ensure_topic respects).
        dms::rejoin_contact_topics(&did).await;
    }
    // Join their feed topic to REACH them (it's the only channel a single scan reaches for the
    // key-bundle event below). A FOLLOW also pulls their feed (sync_req); a CHAT-ONLY connect joins
    // only to deliver the bundle and does NOT request their feed (no feed relationship).
    let topic = feed_topic(&did);
    if ticket.is_empty() {
        let _ = runtime::peer::join_topic(&topic).await;
    } else {
        let _ = runtime::peer::connect(&ticket).await;
        let _ = runtime::peer::join_topic_with(&topic, &[ticket.clone()]).await;
    }
    if !chat_only {
        publish(&topic, "hey-social.sync_req", json!({ "want": "backfill" })).await;
    }
    // F-FOLLOW-ANNOUNCE: deliver OUR DM keys + node ticket over the SEALED DM lane
    // — NOT as a cleartext feed event. The old `hey-social.follow` event carried
    // our X25519 + ML-KEM pubkeys + ticket + name in clear on the public feed
    // topic, leaking the social graph AND our DM-routing keys to any subscriber.
    // The sealed announce (only possible when the follow link gave us their keys,
    // i.e. we now have a v2 channel) hands the followee everything they need to
    // record us as a follower and DM back — privately. A STRIPPED public form
    // (name only, no x/k/ticket) still publishes the "X follows Y" social signal.
    let myk = dms::my_pubkeys().await;
    // F-FOLLOWANNOUNCE-TICKET-LEAK: compact (IP-cap) my node ticket before it rides
    // the sealed announce payload — same treatment my_friend_link gives the public
    // link. Keeps relays + a small set of same-LAN direct hints (MAX_IP_ADDRS=4, an
    // INTENTIONAL relay-less LAN-dial aid, see compact_ticket), drops the rest.
    // NOTE: the announce DM is a KIND_MESSAGE, so hey-core::dms::build_inner_bound
    // ALSO stamps a `nt` field with my ticket — that emitter is independently IP-
    // capped there (compact_nt_ticket), so the full direct-IP set never rides EITHER
    // the payload `ticket` here OR the signed `nt`. Both must stay capped together.
    let myt = compact_ticket(&runtime::peer::my_ticket().await.unwrap_or_default());
    let my_name = stored_profile_nickname().await;
    // F-FOLLOW-ANNOUNCE-LEAK: the sealed announce hands the followee OUR DM keys +
    // node ticket + nickname. For a SIGNED follow link (keys_verified=true, the
    // normal case) the followee's identity+keys are proven, so the announce fires
    // and the follow UX is unchanged. For an UNSIGNED/forged link, those keys are
    // attacker-substitutable AND the announce rides the SOH control lane that is
    // EXEMPT from the needs_verify_before_send gate (dms.rs:3481) — so without this
    // guard a phishing link would leak our routing ticket + nickname under an
    // impersonated DID. Defer the announce until the user clears the contact via
    // verify_contact()/confirm_unverified_send(), which re-fires it then.
    // Anti-phishing gate (F-FOLLOW-ANNOUNCE-LEAK): only auto-send our keys + node ticket to a
    // link we could VERIFY (keys_verified). This is now SAFE on mobile because my_friend_link
    // SIGNS links with the runtime identity, so a legit QR scan is keys_verified=true → the
    // announce fires and pairing completes; a genuinely-forged/unsigned link stays deferred
    // until the user clears the contact via verify_contact/confirm_unverified_send.
    // (5b8e65d added this gate, but back then mobile links were UNSIGNED — so it silently broke
    // ALL mobile pairing. The signing fix above is the precondition that makes this gate correct.)
    let sealed = if keys_verified {
        send_follow_announce(&did, &myt, &my_name, &myk, chat_only).await
    } else {
        false
    };
    // ONE-WAY PAIRING (the fix for "both phones must add each other"): also hand
    // the followee OUR keys + node ticket ENCRYPTED, riding their PUBLIC follow
    // event. Their feed topic is the ONLY channel that reaches them from a single
    // scan — they can't subscribe our per-pair queue yet (they don't know our DID,
    // and the receive path is gated on an existing contact), so the sealed
    // pair-queue announce above is silently undeliverable one-way. Sealing the
    // bundle to THEIR pubkeys keeps it opaque to other feed subscribers, so the
    // social graph + DM keys never leak in clear (the security property 5b8e65d
    // wanted) while a single QR scan now establishes a BIDIRECTIONAL channel.
    let enc: Option<String> = match (keys.as_ref(), myk.as_ref()) {
        (Some((ax, ak)), Some(mk)) => {
            let bundle = json!({
                "x": mk.x25519_pub_b64,
                "k": mk.ml_kem_pub_b64,
                "ticket": myt,
                "name": my_name,
            })
            .to_string();
            dms::seal_bundle_for_peer(
                &dms::PeerKeys { x25519_pub_b64: ax.clone(), ml_kem_pub_b64: ak.clone() },
                &bundle,
            )
            .ok()
        }
        _ => None,
    };
    // Public social-graph signal — cleartext keys/ticket STRIPPED (the sealed `enc`
    // bundle above carries them, readable only by the followee). A keyless
    // feed-only follow carries neither.
    publish(
        &topic,
        "hey-social.follow",
        json!({ "name": my_name, "sealed": sealed, "enc": enc, "chat_only": chat_only }),
    )
    .await;
    Ok(json!({ "ok": true, "did": did }))
}

/// F-FOLLOW-ANNOUNCE: hand a followee OUR DM keys + node ticket + nickname over
/// the SEALED, end-to-end DM lane (a hidden SOH-prefixed control message), so the
/// social graph + our routing keys never ride in clear on the public feed topic.
/// Possible only when we already hold a v2 (sealed) channel to `did` — which we
/// do whenever the follow link carried THEIR keys (we just bootstrapped it). The
/// receiver applies it in `process_sealed_follows` (records us as a follower, can
/// DM back). Returns true if the sealed announce was sent. Falls back silently
/// (returns false) for a keyless feed-only follow — there are no DM keys to share.
async fn send_follow_announce(
    did: &str,
    my_ticket: &str,
    my_name: &str,
    myk: &Option<dms::PeerKeys>,
    // CHAT-ONLY: when true, the receiver bootstraps a DM contact (so we can chat both ways) but
    // does NOT record us as a follower or show "started following you". This is what makes a
    // SCANNED chat invite isolated from follow/feed — same key bundle, different intent.
    chat_only: bool,
) -> bool {
    let Some(k) = myk.as_ref() else {
        return false; // we have no DM keys to share
    };
    // Need a sealed channel: only present when this contact is v2-active (their
    // keys were in the follow link). Otherwise the DM can't be encrypted.
    match dms::find_contact(did).await {
        Some(c) if c.is_v2_active() => {}
        _ => return false,
    }
    let payload = json!({
        "ticket": my_ticket,
        "name": my_name,
        "x": k.x25519_pub_b64,
        "k": k.ml_kem_pub_b64,
        "chat_only": chat_only,
    });
    let b64 = B64U.encode(payload.to_string().as_bytes());
    // chat_send seals + ratchets exactly like any DM; the SOH prefix keeps it
    // hidden from the rendered thread AND exempt from the F-FOLLOW-PoP send gate
    // (it's the very message that establishes mutual DM capability).
    chat_send(did, &format!("{FOLLOW_PREFIX}{b64}")).await.is_ok()
}

/// F-FOLLOW-ANNOUNCE-LEAK: re-fire the sealed follow-announce that `follow()`
/// deferred for an UNSIGNED link. Called when the user clears the contact (safety-
/// number verify or "send anyway") — at that point they've consciously trusted the
/// contact, so it's safe to hand over our DM keys + ticket + nickname. Gathers OUR
/// current keys/ticket/name (same as `follow()`) and delegates to the unchanged
/// `send_follow_announce` (which itself no-ops unless a sealed v2 channel exists),
/// so this never sends to a feed-only/keyless contact and is harmless if a signed
/// follow already announced (the receiver applies each announce idempotently in
/// `record_follower`). Returns true if an announce went out.
async fn deferred_follow_announce(did: &str) -> bool {
    let myk = dms::my_pubkeys().await;
    // F-FOLLOWANNOUNCE-TICKET-LEAK: IP-strip my ticket on the re-fire path too
    // (verify_contact / confirm_unverified_send call this), matching follow().
    let myt = compact_ticket(&runtime::peer::my_ticket().await.unwrap_or_default());
    let my_name = stored_profile_nickname().await;
    // Preserve the original CHAT-ONLY vs FOLLOW intent on the deferred re-fire: chat_from_link
    // never adds to following.json, so "not following them" ⇒ this was a chat-only connect and the
    // re-fired announce must stay chat_only (else the peer would record a follower on verify).
    let following = shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let is_following = following.iter().any(|e| e.get("did").and_then(Value::as_str) == Some(did));
    send_follow_announce(did, &myt, &my_name, &myk, !is_following).await
}

/// Overlay each entry's LIVE nickname (from the cached peer profile) onto
/// `entry["name"]`, mirroring chat_contacts' overlay — so a follower/followee
/// who edits their profile shows their current name in these lists. Falls back
/// to the stored `name` when we hold no cached nickname.
async fn enrich_names(list: Value) -> Value {
    let me = whoami_did().await.unwrap_or_default();
    let mut arr = list.as_array().cloned().unwrap_or_default();
    for e in arr.iter_mut() {
        let Some(did) = e.get("did").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        let prof = raw_profile(&did, &me).await;
        if let Some(n) = prof.get("nickname").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            e["name"] = json!(n);
        }
    }
    json!(arr)
}

/// Collapse duplicate follower records that share a `did`. Returns the deduped list and whether
/// anything changed. A duplicate (e.g. a pending record left behind when an EARLIER copy was
/// accepted, or legacy state from an older build) silently breaks the UI: it inflates the pending
/// badge, and the Activity popup's `LazyColumn(key = did)` throws on the duplicate key. The merge
/// rule is "most-accepted wins": if ANY copy is accepted (pending != true), the survivor is
/// accepted; we keep the newest `ts`, fill missing keys/ticket from any copy, and keep the largest
/// `notified_ts` so we never re-notify for a request already surfaced.
fn dedupe_followers(arr: &[Value]) -> (Vec<Value>, bool) {
    let mut out: Vec<Value> = Vec::with_capacity(arr.len());
    let mut idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut changed = false;
    for e in arr {
        let did = e.get("did").and_then(Value::as_str).unwrap_or("").to_string();
        if did.is_empty() {
            changed = true; // drop a malformed entry
            continue;
        }
        if let Some(&i) = idx.get(&did) {
            changed = true;
            let prev = &mut out[i];
            // accepted wins
            let prev_pending = prev.get("pending").and_then(Value::as_bool).unwrap_or(false);
            let cur_pending = e.get("pending").and_then(Value::as_bool).unwrap_or(false);
            prev["pending"] = json!(prev_pending && cur_pending);
            // newest ts
            let prev_ts = prev.get("ts").and_then(Value::as_i64).unwrap_or(0);
            let cur_ts = e.get("ts").and_then(Value::as_i64).unwrap_or(0);
            if cur_ts > prev_ts {
                prev["ts"] = json!(cur_ts);
            }
            // largest notified_ts (don't re-alert for an already-surfaced request)
            let prev_nt = prev.get("notified_ts").and_then(Value::as_i64).unwrap_or(0);
            let cur_nt = e.get("notified_ts").and_then(Value::as_i64).unwrap_or(0);
            if cur_nt > prev_nt {
                prev["notified_ts"] = json!(cur_nt);
            }
            // fill missing keys/ticket/name from whichever copy has them
            for f in ["x", "k", "ticket", "name"] {
                let has = prev.get(f).and_then(Value::as_str).map(|s| !s.is_empty()).unwrap_or(false);
                if !has {
                    if let Some(v) = e.get(f).filter(|v| !v.is_null()) {
                        prev[f] = v.clone();
                    }
                }
            }
        } else {
            idx.insert(did, out.len());
            out.push(e.clone());
        }
    }
    (out, changed)
}

pub async fn followers() -> Result<Value, String> {
    let list = shared_read_json("hey-social/followers.json").await.ok().flatten().unwrap_or_else(|| json!([]));
    // SELF-HEAL: collapse any same-DID duplicates and persist the canonical list. The UI polls this
    // every few seconds, so a corrupted followers.json (duplicate pending+accepted record) repairs
    // itself on the next read — no restart needed.
    let arr = list.as_array().cloned().unwrap_or_default();
    let (deduped, changed) = dedupe_followers(&arr);
    if changed {
        let _g = storage_lock().lock().await;
        let _ = shared_write_json("hey-social/followers.json", &json!(deduped)).await;
        log::info!("followers: healed {} duplicate/malformed record(s)", arr.len() - deduped.len());
    }
    Ok(enrich_names(json!(deduped)).await)
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
    let r = follow(&link).await;
    // PRIVATE FEED: following back is an explicit accept of this follower — hand them my current
    // feed key so they can open my sealed posts (self-gates if the contact is still verify-gated).
    send_feed_key_to(did).await;
    r
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
                let name = r.get("name").and_then(Value::as_str).unwrap_or("");
                bootstrap_dm(did, name, &Some((x.to_string(), k.to_string())), ticket).await;
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

/// SOFT delete (user intent: "delete all local data for this chat; they can still message me"):
/// wipe the conversation history + hide it, but KEEP the contact + queues so a future message
/// re-opens it. NOT a block. Both entrypoints below map to the same soft delete.
pub async fn delete_conversation(did: &str) -> Result<Value, String> {
    dms::hide_conversation(did).await?;
    Ok(json!({ "ok": true }))
}
pub async fn delete_chat(did: &str) -> Result<Value, String> {
    dms::hide_conversation(did).await?;
    Ok(json!({ "ok": true }))
}
pub async fn delete_group(gid: &str) -> Result<Value, String> {
    dms::delete_group(gid).await.map(|_| json!({ "ok": true }))
}

/// SAFETY-NUMBER VERIFICATION: mark this contact's pinned keys verified (the
/// user compared the safety number out-of-band) and clear any key_changed alarm.
/// Surfaces in chat_contacts as key_verified=true / key_changed=false.
pub async fn verify_contact(did: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    let r = dms::verify_contact(did).await.map(|_| json!({ "ok": true }));
    // F-FOLLOW-ANNOUNCE-LEAK: the user just trusted this contact, so deliver the
    // sealed follow-announce that follow() deferred for an unsigned link. No-op if
    // there's nothing to send (no v2 channel / already announced).
    if r.is_ok() {
        let _ = deferred_follow_announce(did).await;
    }
    r
}

/// F-FOLLOW-PoP: the user chose "send anyway" on a contact whose keys came from
/// an unverified, unsigned follow link. Clears ONLY the first-send gate (keys
/// stay pinned UNVERIFIED). After this, send_message no longer returns the
/// `needs_verify_before_send` sentinel for this contact. Surfaces in
/// chat_contacts as needs_verify_before_send=false.
pub async fn confirm_unverified_send(did: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    let r = dms::confirm_unverified_send(did).await.map(|_| json!({ "ok": true }));
    // F-FOLLOW-ANNOUNCE-LEAK: "send anyway" means the user accepted this contact,
    // so deliver the sealed follow-announce that follow() deferred for an unsigned
    // link. No-op if there's nothing to send (no v2 channel / already announced).
    if r.is_ok() {
        let _ = deferred_follow_announce(did).await;
    }
    r
}
/// F-12: keys-based safety number for an ESTABLISHED contact (Signal-style —
/// hashes BOTH parties' pinned encryption material). Returns "" when either
/// side's pinned keys aren't known yet (legacy/keyless contact), letting the UI
/// fall back to its DID-only fingerprint so the ceremony never goes blank.
pub async fn safety_number(did: &str) -> String {
    ensure_session().await.ok();
    dms::safety_number(did).await
}
/// ADMIN "delete group for everyone" — creator-only (enforced by the engine).
/// Fans a signed DISSOLVE to every member, then deletes locally + tombstones.
pub async fn chat_dissolve_group(gid: &str) -> Result<Value, String> {
    dms::dissolve_group(gid).await.map(|_| json!({ "ok": true }))
}
pub async fn chat_accept_group(gid: &str) -> Result<Value, String> {
    dms::accept_group(gid).await.map(|_| json!({ "ok": true }))
}
pub async fn chat_decline_group(gid: &str) -> Result<Value, String> {
    dms::decline_group(gid).await.map(|_| json!({ "ok": true }))
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
    // Drop their cached feed posts + my held feed key so unfollowing actually clears their content
    // from my feed (not just stops new posts).
    purge_author_feed(did).await;
    // Tell the (ex-)followee we unfollowed so they drop us from THEIR followers + rekey — otherwise
    // their followers list shows us forever and a later re-follow looks like an existing relationship
    // (no fresh "wants to follow you" request). Sealed control DM, same lane as the follow-announce;
    // best-effort — if no sealed channel exists it no-ops (their next prune cleans the stale record).
    let _ = chat_send(did, &format!("{UNFOLLOW_PREFIX}{{}}")).await;
    Ok(json!({ "ok": true }))
}

pub async fn following() -> Result<Value, String> {
    let list = shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| json!([]));
    Ok(enrich_names(list).await)
}

/// Remove a follower: drop them from followers.json, PURGE their cached feed posts, and rotate the
/// feed epoch (forward secrecy). Does NOT block — a removed follower can re-follow / be re-added
/// (reversible). The punitive "remove + block" lives in `block_follower`. Returns {"ok":true}.
pub async fn remove_follower(did: &str) -> Result<Value, String> {
    // Scope the storage lock so it is RELEASED before purge/rekey (which re-acquire it) —
    // storage_lock is NOT re-entrant.
    let removed;
    {
        let _g = storage_lock().lock().await;
        let mut list = shared_read_json("hey-social/followers.json")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let before = list.len();
        list.retain(|e| e.get("did").and_then(Value::as_str) != Some(did));
        removed = list.len() != before;
        if removed {
            shared_write_json("hey-social/followers.json", &json!(list))
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    // ONLY do the expensive purge + epoch-bump/rekey when we ACTUALLY removed a follower. Otherwise a
    // re-delivered UNFOLLOW (re-scanned from a contact's recent-message window after a process restart,
    // since the follow_seen dedup is in-memory only) would bump my feed epoch + rekey EVERY remaining
    // follower for nothing — a rekey storm. Gating on `removed` makes a no-op removal truly idempotent.
    if removed {
        // Drop their cached feed posts so they don't linger in my feed after removal.
        purge_author_feed(did).await;
        // PRIVATE FEED forward-secrecy: bump my epoch + re-key remaining followers so the removed
        // follower's old key can't open any post I publish (or re-broadcast on backfill) from now on.
        rekey_remaining_followers().await;
    }
    Ok(json!({ "ok": true }))
}

/// FOLLOW-REQUEST accept: grant a PENDING follower access to my feed — clear their `pending` flag,
/// bootstrap a sealed DM channel from the keys they shared on follow, and hand them my current feed
/// key so they can open my sealed posts. Does NOT make me follow them back (that's follow_back).
/// send_feed_key_to self-gates: if the follower shared no PQ keys (a bare did:key follow) there is
/// no sealed channel to deliver the key on, so they get public visibility only until a real
/// (key-bearing) pairing exists. Idempotent.
pub async fn accept_follower(did: &str) -> Result<Value, String> {
    let rec = {
        let _g = storage_lock().lock().await;
        let mut list = shared_read_json("hey-social/followers.json")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let mut found = None;
        // Clear pending on EVERY record for this DID (not just the first) so a stray duplicate can
        // never leave the follower stuck on "Requested" after Accept. dedupe_followers collapses
        // them on the next read; this is the belt to that suspenders.
        for e in list.iter_mut() {
            if e.get("did").and_then(Value::as_str) == Some(did) {
                e["pending"] = json!(false);
                found = Some(e.clone());
            }
        }
        if found.is_some() {
            shared_write_json("hey-social/followers.json", &json!(list)).await.ok();
        }
        found
    };
    let Some(r) = rec else {
        return err("not a pending follower");
    };
    // Bootstrap the sealed DM channel from the follower's shared keys (needed to deliver the key).
    if let (Some(x), Some(k)) =
        (r.get("x").and_then(Value::as_str), r.get("k").and_then(Value::as_str))
    {
        let ticket = r.get("ticket").and_then(Value::as_str).unwrap_or("");
        let name = r.get("name").and_then(Value::as_str).unwrap_or("");
        bootstrap_dm(did, name, &Some((x.to_string(), k.to_string())), ticket).await;
        // ISOLATION: accepting a FOLLOW request is a FEED action — it bootstraps the DM only to
        // carry the feed key, so it must NOT pop an empty chat row in the chat list. Hide it until a
        // real message (either side) un-hides it. (Was unhide_chat, which surfaced the empty chat.)
        dms::hide_chat_if_empty(did).await;
        dms::rejoin_contact_topics(did).await;
        // ACCEPT = explicit consent. bootstrap_dm creates the contact UNVERIFIED, which arms the
        // first-send gate (needs_verify_before_send=true) — so send_feed_key_to would fail closed
        // and the accepted follower would stay stuck on "Requested", never getting the feed key.
        // Tapping Accept IS the user's consent to send, so clear ONLY the send gate here. The keys
        // stay pinned UNVERIFIED (key_verified=false → the safety-number-changed alarm still fires
        // later), and confirm_unverified_send REFUSES if key_changed, preserving MITM protection.
        let _ = dms::confirm_unverified_send(did).await;
    } else {
        // Genuine anomaly: a pending follower with no shared PQ keys (e.g. a bare did:key
        // follow) has no sealed channel to deliver the feed key on → public visibility only.
        log::warn!("accept_follower: pending follower {did} has no PQ keys — public visibility only");
    }
    let _ = send_feed_key_to(did).await;
    Ok(json!({ "ok": true }))
}

/// FOLLOW-REQUEST reject: drop a PENDING follow request. No epoch re-key needed — a pending
/// follower never received the feed key, so there is no forward-secrecy exposure to close.
pub async fn reject_follower(did: &str) -> Result<Value, String> {
    let _g = storage_lock().lock().await;
    let mut list = shared_read_json("hey-social/followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let before = list.len();
    list.retain(|e| e.get("did").and_then(Value::as_str) != Some(did));
    if list.len() != before {
        shared_write_json("hey-social/followers.json", &json!(list))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(json!({ "ok": true }))
}

/// Clear the `pending` flag on a following.json entry once the author's feed key arrives (proof
/// they accepted) — flips the follower-side UI from "Requested" to "Following". Idempotent.
async fn flip_following_pending_off(author: &str) {
    let _g = storage_lock().lock().await;
    let mut list = shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut changed = false;
    for e in list.iter_mut() {
        if e.get("did").and_then(Value::as_str) == Some(author)
            && e.get("pending").and_then(Value::as_bool).unwrap_or(false)
        {
            e["pending"] = json!(false);
            changed = true;
            break;
        }
    }
    if changed {
        let _ = shared_write_json("hey-social/following.json", &json!(list)).await;
    }
}

/// Remove a follower AND block them: they can no longer follow / DM you until you unblock (Settings
/// → Blocked users). The punitive action behind the explicit "Block" button — kept DISTINCT from
/// the plain, reversible `remove_follower` so a routine cleanup never silently blocks someone.
pub async fn block_follower(did: &str) -> Result<Value, String> {
    remove_follower(did).await?;
    let _g = storage_lock().lock().await;
    let mut blocked = shared_read_json("hey-social/blocked-followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    if !blocked.iter().any(|b| b.as_str() == Some(did)) {
        blocked.push(json!(did));
        shared_write_json("hey-social/blocked-followers.json", &json!(blocked))
            .await
            .map_err(|e| e.to_string())?;
    }
    drop(_g);
    // BLOCK is the deliberate SEVER (vs delete, which is soft/recoverable): revoke chat capability
    // so even after an unblock the peer must re-establish via a chat QR. FOLLOW≠CHAT throughout.
    dms::disable_chat(did).await;
    // UI-ONLY courtesy signal (best-effort): tell the blocked peer over the sealed channel so their
    // app can show "you've been blocked" + disable the composer. No-op if no sealed channel exists.
    let _ = chat_send(did, &format!("{BLOCK_PREFIX}{{}}")).await;
    Ok(json!({ "ok": true }))
}

/// Purge a user's cached FEED data locally (called on unfollow / remove): delete their posts (+ the
/// reactions/comments filed under each post id) from the feed index + store, and drop the feed key
/// I held for them so no lingering or future sealed post of theirs can be opened. Leaves their chat
/// + profile-name cache intact (chat is a separate relationship). Takes storage_lock — callers must
/// NOT hold it.
async fn purge_author_feed(did: &str) {
    let _g = storage_lock().lock().await;
    let idx: Vec<Value> = shared_read_json(FEED_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut kept: Vec<Value> = Vec::with_capacity(idx.len());
    for id in idx {
        let pid = match id.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let author = shared_read_json(&format!("hey-social/posts/{pid}.json"))
            .await
            .ok()
            .flatten()
            .and_then(|p| p.get("author").and_then(Value::as_str).map(str::to_string));
        if author.as_deref() == Some(did) {
            let _ = shared_write_json(&format!("hey-social/posts/{pid}.json"), &Value::Null).await;
            let _ = shared_write_json(&format!("hey-social/reactions/{pid}.json"), &Value::Null).await;
            let _ = shared_write_json(&format!("hey-social/comments/{pid}.json"), &Value::Null).await;
        } else {
            kept.push(id);
        }
    }
    let _ = shared_write_json(FEED_INDEX, &json!(kept)).await;
    // Drop the feed key I hold for this author so nothing of theirs decrypts after removal.
    let _ = shared_write_json(&format!("hey-social/feed-keys/{did}.json"), &Value::Null).await;
}

/// True if `did` is on the removed/blocked-followers list — checked before we
/// record an inbound "hey-social.follow" so a removed follower stays removed.
async fn is_blocked_follower(did: &str) -> bool {
    shared_read_json("hey-social/blocked-followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|b| b.as_str() == Some(did))
}

/// True only if `did` is a CURRENT, ACCEPTED follower (present in followers.json and not pending).
/// Used to gate media (blob_req) serving: in the private-account model my media is followers-only,
/// so a follower I REMOVED (or never accepted) must not be able to keep pulling my media bytes by a
/// CID they learned earlier. Pending/removed/stranger → false.
async fn is_accepted_follower(did: &str) -> bool {
    shared_read_json("hey-social/followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|e| {
            e.get("did").and_then(Value::as_str) == Some(did)
                && e.get("pending").and_then(Value::as_bool) != Some(true)
        })
}

/// True if `did` (a 1:1 contact) has sent us a BLOCK signal (recorded in blocked-by-peer.json by
/// process_sealed_follows) and not yet an UNBLOCK. UI-only: lets the chat screen show "you've been
/// blocked" + disable the composer. Does NOT affect what we can/can't send at the engine level.
pub async fn is_blocked_by_peer(did: &str) -> bool {
    shared_read_json("hey-social/blocked-by-peer.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|b| b.as_str() == Some(did))
}

/// Unblock `did`: the inverse of remove_follower's block step — drop it from
/// blocked-followers.json so they may follow/DM again. Does NOT re-add them as a
/// follower (they'd have to follow again). Returns {"ok":true}.
pub async fn unblock_follower(did: &str) -> Result<Value, String> {
    let _g = storage_lock().lock().await;
    let mut blocked = shared_read_json("hey-social/blocked-followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    blocked.retain(|b| b.as_str() != Some(did));
    shared_write_json("hey-social/blocked-followers.json", &json!(blocked))
        .await
        .map_err(|e| e.to_string())?;
    drop(_g);
    // UI-ONLY courtesy signal (best-effort): tell the peer they're unblocked so their app re-enables
    // the composer. No-op if no sealed channel exists.
    let _ = chat_send(did, &format!("{UNBLOCK_PREFIX}{{}}")).await;
    Ok(json!({ "ok": true }))
}

/// The blocked-followers list as `[{ "did":..., "name":... }]` for the settings
/// screen, resolving each DID's live display name from the cached peer profile
/// (the same source chat_contacts/enrich_names use) and falling back to a short
/// DID label when we hold no nickname.
pub async fn list_blocked() -> Value {
    let me = whoami_did().await.unwrap_or_default();
    let blocked = shared_read_json("hey-social/blocked-followers.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for b in &blocked {
        let Some(did) = b.as_str() else { continue };
        let prof = raw_profile(did, &me).await;
        let name = prof
            .get("nickname")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| did.trim_start_matches("did:key:z").chars().take(10).collect());
        out.push(json!({ "did": did, "name": name }));
    }
    json!(out)
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

/// Self-healing prune: leave any carrier subscription with NO backing record. Desired set is
/// RECORD-driven (my feed + followed feeds + every DM contact's feed topic + dms::my_v2_topics),
/// never neighbor-driven, so an offline contact (record present, 0 neighbors) is NEVER pruned —
/// only true orphans.
pub async fn prune_orphan_topics() {
    let me = match whoami_did().await {
        Ok(d) => d,
        Err(_) => return,
    };
    let mut desired: std::collections::HashSet<String> = std::collections::HashSet::new();
    desired.insert(feed_topic(&me));
    for did in followed_dids().await {
        desired.insert(feed_topic(&did));
    }
    // A chat-only contact's feed topic is NOT in following.json and differs from the q/* DM
    // queues, but it carries the sealed key bundle during pairing — pruning it can permanently
    // strand the bundle. Keep every DM contact's feed topic in the desired set.
    for c in dms::list_contacts().await {
        desired.insert(feed_topic(&c.did));
    }
    for (topic, _c, _b) in dms::my_v2_topics().await {
        desired.insert(topic);
    }
    let resp = match runtime::peer::list_subscriptions().await {
        Ok(v) => v,
        Err(_) => return,
    };
    let current = resp
        .get("data")
        .and_then(|d| d.get("topics"))
        .or_else(|| resp.get("topics"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if desired.len() <= 1 && current.len() > 1 {
        return;
    } // safety valve: never prune on an empty/transient read
    for t in current {
        if let Some(topic) = t.as_str() {
            if !desired.contains(topic) {
                let _ = runtime::peer::leave_topic(topic).await;
            }
        }
    }
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
    // CHAT-ISOLATION MIGRATION (one-time, marker-guarded): seed the chat-enabled set from every
    // contact that already has real history, so pre-existing private chats stay chattable under the
    // new set-only rule. Must run at boot before any send could be gated.
    dms::migrate_chat_enabled_from_history().await;
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
    // DATA-INTEGRITY: collapse any duplicate-DID contacts left by legacy re-pair / mutual-invite
    // cycles, so find_contact + ratchet routing always resolve the same record (heals the data the
    // chat_contacts read-side dedup was only papering over).
    dms::compact_contacts().await;
    // Soft-deleted chats are NOT skipped here: delete-chat only wipes the local
    // history + hides the row (see dms::hide_conversation); it KEEPS the contact +
    // queues so a new message from either side re-opens the thread. The hidden flag
    // lives in dms/hidden-chats.json and is filtered at the chat_contacts list layer
    // (un-hidden by dms::unhide_chat on an explicit re-add or any inbound message).
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
            let name = e.get("name").and_then(Value::as_str).unwrap_or("");
            if let (Some(x), Some(k)) =
                (e.get("x").and_then(Value::as_str), e.get("k").and_then(Value::as_str))
            {
                bootstrap_dm(did, name, &Some((x.to_string(), k.to_string())), ticket).await;
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
    // F-FOLLOW-ANNOUNCE: apply any sealed follow announces that the DM receiver
    // stored since the last tick (records the follower + bootstraps the DM
    // contact). Runs every tick so a follower is recorded without opening the chat.
    process_sealed_follows().await;
    // Re-drive any accepted follower whose feed key isn't yet ACK-confirmed (backoff-bounded,
    // idempotent on the receiver) — recovers an Ok-but-undelivered send across a mesh blip/restart.
    retry_pending_feed_keys().await;
    // Drive the DM / group OUTBOX on the same 2s tick. A message that broadcast to 0
    // neighbors (sent mid-handoff, or before the mesh formed) is queued with exponential
    // backoff; without a timer-driven flush it would only retry on the user's NEXT
    // send/receive action. Flushing here re-attempts every due item whenever the device
    // is online + meshed — so an undelivered message lands as soon as the peer is
    // reachable again, no user action required. flush() is backoff-gated + idempotent.
    hey_core::api::outbox::flush().await;
    // Fix 3: throttled self-healing prune of orphaned carrier subscriptions (no backing record).
    {
        use std::sync::atomic::{AtomicI64, Ordering};
        static LAST_PRUNE_MS: AtomicI64 = AtomicI64::new(0);
        let now = hey_core::plat::now_ms();
        if now - LAST_PRUNE_MS.load(Ordering::Relaxed) > 120_000 {
            LAST_PRUNE_MS.store(now, Ordering::Relaxed);
            prune_orphan_topics().await;
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
    // Reject an oversized event before parsing/buffering anything (a hostile
    // signer on a followed topic can't force us to store a huge post/comment).
    // The `blob` chunk path has its own (larger) per-chunk ceiling, so exempt it
    // here and let its handler enforce BLOB_CHUNK precisely.
    if wire.len() > MAX_INGEST_PAYLOAD && !wire.contains("hey-social.blob\"") {
        return false;
    }
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
            // PRIVATE FEED: a sealed post (`enc`) carries all content inside `sealed` — open it with
            // the author's epoch key. No key yet (not an accepted follower, or a pre-rotation epoch)
            // → DROP it; once the key arrives, cache_feed_key pulls a backfill that re-delivers it.
            // A post with no `sealed` field is legacy cleartext (backward compatible).
            let post = if ev.payload.get("sealed").is_some() {
                match open_sealed_post(&ev.payload).await {
                    Some(p) => p,
                    None => return false,
                }
            } else {
                ev.payload.clone()
            };
            let key = format!("hey-social/posts/{id}.json");
            let prev = shared_read_json(&key).await.ok().flatten();
            let existed = prev.is_some();
            // FRESHNESS GATE: reject a stale REPLAY that would roll back an edit. edited_ts is
            // monotonic (create sets it = ts, edit_post bumps it to now_ms); never overwrite a newer
            // stored post with an older incoming one. Falls back to ts for legacy posts.
            let rev = |p: &Value| {
                p.get("edited_ts").or_else(|| p.get("ts")).and_then(Value::as_i64).unwrap_or(0)
            };
            if let Some(prev_post) = prev.as_ref() {
                if rev(&post) < rev(prev_post) {
                    return false; // stale replay — keep the newer version
                }
            }
            // Store the DECRYPTED post (disk is DEK-sealed at rest) so the read/render path is
            // unchanged and never sees ciphertext.
            let _ = shared_write_json(&key, &post).await;
            add_to_index(&id).await;
            // LAZY MEDIA: do NOT eager-fetch the post's photos/videos here. A backfill of many
            // posts would pull every image at once and fill storage + bandwidth. The bytes are
            // fetched ON DEMAND when a tile scrolls into view (hey_ensure_media → ensure_blob),
            // and the feed card shows a sleek loading state until they arrive. The author's
            // avatar is tiny + shown widely (feed, chat, profile), so keep IT eager.
            if let Some(cid) = post.get("author_avatar").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                ensure_blob(cid, author).await;
            }
            // Cache the author's denormalized name into their profile cache so CHAT
            // (chat_contacts' overlay) shows the real nickname too, not just the feed.
            if let Some(an) = post.get("author_name").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                seed_peer_nickname(author, an).await;
            }
            if !existed {
                let name = post.get("author_name").and_then(Value::as_str).filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| author.trim_start_matches("did:key:z").chars().take(10).collect());
                // A post that names you gets a louder "mentioned you" notification.
                // key = post id so two distinct posts by one author don't collapse.
                if mentions_me(&post, me).await {
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
                // FOLLOWER-GATE (revocation): my media is followers-only, so serve a blob_req ONLY
                // to a CURRENT accepted follower. A removed or blocked follower (or a stranger who
                // somehow learned a CID) gets nothing — closing the gap where a removed follower
                // could keep pulling my media bytes by a CID they decrypted while still a follower.
                if is_blocked_follower(&ev.sender_did).await
                    || !is_accepted_follower(&ev.sender_did).await
                {
                    return false;
                }
                if let Some(cid) = ev.payload.get("cid").and_then(Value::as_str) {
                    // F-CONTENT-CID-NOSCOPE: serve ONLY a CID that belongs to my
                    // PUBLIC surface (my avatar / my own posts' media). Without this
                    // a removed group member could blob_req a PRIVATE group-avatar
                    // CID I still hold and I'd re-serve it. Refuse anything else
                    // (fail closed) — a real follower only ever requests CIDs that
                    // appear in my public posts/profile, so legit fetch is unchanged.
                    if !is_public_media_cid(cid, me).await {
                        return false;
                    }
                    // Per-sender cooldown (amplification gate) AND per-cid
                    // coalescing: a re-chunk is expensive, so don't re-serve the
                    // same media to the same requester (or any requester for the
                    // same cid) more than once per cooldown window.
                    let sender_ok = req_cooldown_ok(&ev.sender_did, "blob_req");
                    let cid_ok = req_cooldown_ok(cid, "blob_serve");
                    if sender_ok && cid_ok {
                        if let Ok(bytes) = content::get_bytes(cid, None).await {
                            // F-CONTENT-CID-NOSCOPE: UNICAST the response to the
                            // requester's OWN feed topic (which they always sub to)
                            // instead of broadcasting on my public feed topic — so
                            // the media doesn't fan out to every one of my
                            // followers. The receiver gates the blob on a content
                            // hash + a prior ensure_blob request, so this reaches
                            // only the asker. We also publish on my own topic so any
                            // OTHER follower with a pending request for the SAME
                            // public CID still backfills (legacy/broadcast path,
                            // preserved for public media).
                            send_blob(cid, &bytes, &feed_topic(&ev.sender_did)).await;
                            send_blob(cid, &bytes, topic).await;
                        }
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
            // SOLICITED-GATE: only buffer chunks for a cid WE actually requested
            // (ensure_blob records it in requested_blobs). An unsolicited blob on a
            // followed topic — the flood vector — is dropped before any allocation.
            // requested_blobs is read-fallback safe: a legit transfer always has an
            // entry because ensure_blob runs first (post/profile ingest path).
            if !crate::lock_safe(requested_blobs()).contains_key(&cid) {
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
                // Cap concurrent reassemblies by COUNT before pre-allocating a new
                // entry's n slots: evict the oldest if a new cid would exceed the
                // ceiling (mirrors frag.rs MAX_PARTIALS).
                if !p.contains_key(&cid) {
                    while p.len() >= MAX_PENDING_CIDS {
                        if !evict_oldest_pending(&mut p) {
                            break;
                        }
                    }
                }
                let entry = p.entry(cid.clone()).or_insert_with(|| (now, vec![None; n]));
                // A re-chunk (different n across senders/versions) resets the buffer
                // instead of deadlocking.
                if entry.1.len() != n {
                    *entry = (now, vec![None; n]);
                }
                entry.1[i] = data;
                let complete = entry.1.iter().all(|c| c.is_some());
                // Cap total buffered BYTES across all reassemblies: if this chunk
                // pushed us over budget, evict oldest entries (but never the one we
                // just completed — that's removed below). Bounds the byte cost of a
                // many-distinct-cid flood even when each is under the count cap.
                while pending_blob_bytes(&p) > MAX_PENDING_BYTES && p.len() > 1 {
                    // Don't evict the cid we're about to finalize.
                    let victim = p
                        .iter()
                        .filter(|(k, _)| !(complete && *k == &cid))
                        .min_by_key(|(_, (t, _))| *t)
                        .map(|(k, _)| k.clone());
                    match victim {
                        Some(k) => {
                            p.remove(&k);
                        }
                        None => break,
                    }
                }
                complete
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
            let unset = ev.payload.get("op").and_then(Value::as_str) == Some("unset");
            if post_id.is_empty() {
                return false;
            }
            // Bind the reaction to the POST AUTHOR's feed topic — react() publishes
            // it there. Without this, any signer on a topic we follow could inject
            // reactions onto arbitrary post_ids in our local view. Drop reactions
            // that arrive on the wrong topic or for a post we don't actually hold.
            let post_author = match get_post(post_id).await {
                Ok(p) => {
                    let author = p.get("author").and_then(Value::as_str).unwrap_or("").to_string();
                    if author.is_empty() || topic != feed_topic(&author) {
                        return false;
                    }
                    author
                }
                Err(_) => return false,
            };
            // PRIVATE FEED: the emoji is sealed under the post author's feed key. Open it; no key /
            // tamper → drop. A legacy cleartext reaction (no `sealed`) still applies.
            let emoji = if let Some(sealed) = ev.payload.get("sealed").and_then(Value::as_str) {
                match open_for_feed(&post_author, sealed).await {
                    Some(c) => c.get("emoji").and_then(Value::as_str).unwrap_or("❤️").to_string(),
                    None => return false,
                }
            } else {
                ev.payload.get("emoji").and_then(Value::as_str).unwrap_or("❤️").to_string()
            };
            let key = format!("hey-social/reactions/{post_id}.json");
            let _g = storage_lock().lock().await;
            // REPLAY GATE: a reaction is a validly-signed event, so a topic member can capture an
            // old set/unset and replay it to roll back another user's current reaction state.
            // Track the last-applied signed ts PER REACTOR in a sidecar and reject any event that
            // isn't strictly newer (legit toggles carry a fresh now_ms ts, so they always pass; a
            // replay/stale event repeats an old ts and is dropped). Sidecar keeps the ts even after
            // an unset, so a replayed older `set` can't re-add a reaction the user later removed.
            let seen_key = format!("hey-social/reactions/{post_id}.seen.json");
            let mut seen = shared_read_json(&seen_key)
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            let prev_ts = seen.get(&ev.sender_did).and_then(Value::as_i64).unwrap_or(0);
            if (ev.ts as i64) <= prev_ts {
                return false; // replay or stale — never roll back current reaction state
            }
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
            seen.insert(ev.sender_did.clone(), json!(ev.ts));
            let _ = shared_write_json(&seen_key, &Value::Object(seen)).await;
            // Notify when SOMEONE ELSE likes MY post (a fresh set, not an un-like). The replay
            // gate above guarantees we only get here for a genuinely new reaction.
            if !unset && post_author == me && ev.sender_did != me {
                let name = peer_display_name(&ev.sender_did).await;
                push_notif("like", &name, &format!("reacted {emoji} to your post"), &ev.sender_did, post_id);
            }
            true
        }
        "hey-social.comment" => {
            let post_id = ev.payload.get("post_id").and_then(Value::as_str).unwrap_or("").to_string();
            let comment = ev.payload.get("comment").cloned().unwrap_or(Value::Null);
            let cauthor = comment.get("author").and_then(Value::as_str).unwrap_or("").to_string();
            let cid = comment.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            // Comment author must be the signer.
            if post_id.is_empty() || cid.is_empty() || cauthor != ev.sender_did {
                return false;
            }
            // Bind to the POST AUTHOR's feed topic (where add_comment() publishes),
            // and only for posts we hold — otherwise any signer on a followed topic
            // could inject comments onto arbitrary post_ids.
            let post_author = match get_post(&post_id).await {
                Ok(p) => {
                    let author = p.get("author").and_then(Value::as_str).unwrap_or("").to_string();
                    if author.is_empty() || topic != feed_topic(&author) {
                        return false;
                    }
                    author
                }
                Err(_) => return false,
            };
            // PRIVATE FEED: the comment CONTENT (author_name + text) is sealed under the post
            // author's feed key. Merge the decrypted content with the cleartext routing fields; no
            // key / tamper → drop. A legacy cleartext comment (no `sealed`) applies as-is.
            let comment_full = if let Some(sealed) = comment.get("sealed").and_then(Value::as_str) {
                match open_for_feed(&post_author, sealed).await {
                    Some(secret) => json!({
                        "id": cid,
                        "author": cauthor,
                        "ts": comment.get("ts").cloned().unwrap_or(Value::Null),
                        "parent": comment.get("parent").cloned().unwrap_or(Value::Null),
                        "author_name": secret.get("author_name").cloned().unwrap_or_default(),
                        "text": secret.get("text").cloned().unwrap_or_default(),
                    }),
                    None => return false,
                }
            } else {
                comment.clone()
            };
            let key = format!("hey-social/comments/{post_id}.json");
            let _g = storage_lock().lock().await;
            // REPLAY-AFTER-EVICTION GATE: the in-list id dedup below FORGETS any comment evicted
            // once the post passes MAX_COMMENTS_PER_POST, so a follower could replay an evicted
            // (validly-signed) comment to re-add it. Track a per-AUTHOR high-water ts in a sidecar
            // (bounded to #authors, mirrors the reactions .seen.json) that SURVIVES eviction; reject
            // any comment from that author not strictly newer. Legit comments carry a fresh now_ms
            // ts and a given author authors them sequentially, so this never drops a real comment.
            let cts = comment.get("ts").and_then(Value::as_i64).unwrap_or(0);
            let seen_key = format!("hey-social/comments/{post_id}.seen.json");
            let mut seen = shared_read_json(&seen_key)
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            let prev_ts = seen.get(&cauthor).and_then(Value::as_i64).unwrap_or(0);
            if cts != 0 && cts <= prev_ts {
                return false; // replay/stale (legacy ts==0 comments skip the gate, back-compat)
            }
            let mut list = shared_read_json(&key)
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            // Set-backed dedup (O(1)) instead of an O(n) scan per ingest.
            let ids: std::collections::HashSet<&str> =
                list.iter().filter_map(|c| c.get("id").and_then(Value::as_str)).collect();
            if ids.contains(cid.as_str()) {
                return false; // dedupe
            }
            drop(ids);
            // Capture the commenter's display name before comment_full is moved into the list.
            let cname = comment_full.get("author_name").and_then(Value::as_str)
                .filter(|s| !s.is_empty()).map(String::from);
            list.push(comment_full);
            // Bound the per-post comment list: keep the most recent. Backward-
            // compatible — an over-cap legacy list is trimmed on the next ingest.
            if list.len() > MAX_COMMENTS_PER_POST {
                let drop_n = list.len() - MAX_COMMENTS_PER_POST;
                list.drain(0..drop_n);
            }
            let _ = shared_write_json(&key, &json!(list)).await;
            if cts > prev_ts {
                seen.insert(cauthor.clone(), json!(cts));
                let _ = shared_write_json(&seen_key, &Value::Object(seen)).await;
            }
            // Notify when SOMEONE ELSE comments on MY post.
            if post_author == me && cauthor != me {
                let name = match cname { Some(n) => n, None => peer_display_name(&cauthor).await };
                push_notif("comment", &name, "commented on your post", &cauthor, &post_id);
            }
            true
        }
        "hey-social.sync_req" => {
            // Only the topic OWNER answers (re-announce my recent posts here).
            // Per-sender cooldown: a backfill re-broadcasts my whole feed, so a
            // peer can't spam sync_req to make me flood the topic. The first
            // request in each window still backfills the (legit) new follower.
            if topic == feed_topic(me) && req_cooldown_ok(&ev.sender_did, "sync_req") {
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
            // SECURITY (F-04): NEVER cache `addresses` off the PUBLIC feed topic.
            // The wallet address map arrives only over the sealed DM (cache_peer_addresses).
            // PRESERVE any privately-cached addresses so a public profile update
            // (nickname/bio/avatar) doesn't wipe the DM-shared tip addresses, and a
            // legacy peer still broadcasting addresses publicly is silently ignored.
            let prev_addresses = shared_read_json(&path).await.ok().flatten()
                .and_then(|v| v.get("addresses").cloned()).unwrap_or(Value::Null);
            // PRIVATE FEED: the profile content is sealed under the OWNER's (signer's) feed key —
            // only accepted followers (who hold their key) can read it. No key / tamper → drop. A
            // legacy cleartext profile (no `sealed`) applies as-is. The chat-name path is seeded
            // separately (follow announce + post author_name), so this never blanks chat display.
            let content = if let Some(sealed) = ev.payload.get("sealed").and_then(Value::as_str) {
                match open_for_feed(&ev.sender_did, sealed).await {
                    Some(c) => c,
                    None => return false,
                }
            } else {
                ev.payload.clone()
            };
            let p = json!({
                "nickname": content.get("nickname").and_then(Value::as_str).unwrap_or(""),
                "bio": content.get("bio").and_then(Value::as_str).unwrap_or(""),
                "avatar": content.get("avatar").and_then(Value::as_str).unwrap_or(""),
                "addresses": prev_addresses,
                "ts": ev.ts,
            });
            let _ = shared_write_json(&path, &p).await;
            // Pull the avatar bytes so it renders.
            if let Some(cid) = content.get("avatar").and_then(Value::as_str).filter(|s| !s.is_empty()) {
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
            // Someone (the signer) followed us. F-FOLLOW-ANNOUNCE: the public form
            // is now STRIPPED — name only, no x/k/ticket (those ride the sealed DM
            // lane, applied by process_sealed_follows). A peer running the OLD
            // client may still send keys here; record_follower handles both. Only
            // meaningful on OUR topic.
            if topic != feed_topic(me) {
                return false;
            }
            let follower = ev.sender_did.clone();
            let mut ticket = ev.payload.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
            let mut follower_name = ev.payload.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            // Legacy cleartext keys (a peer on the OLD client may still send these).
            let mut keys = match (ev.payload.get("x").and_then(Value::as_str), ev.payload.get("k").and_then(Value::as_str)) {
                (Some(x), Some(k)) => Some((x.to_string(), k.to_string())),
                _ => None,
            };
            // ONE-WAY PAIRING: the new client puts our key-share ENCRYPTED in `enc`
            // (sealed to OUR pubkeys) instead of in clear. Decrypting it proves the
            // sender held our pubkeys (from our QR/friend-link) — an INVITED pairing,
            // not spam — so we treat it as TRUSTED and materialize the DM contact
            // even though we never followed them. This is what makes a single scan
            // establish a bidirectional channel.
            let mut trusted = false;
            if keys.is_none() {
                if let Some(enc) = ev.payload.get("enc").and_then(Value::as_str) {
                    if let Ok(pt) = dms::open_bundle_for_me(enc).await {
                        if let Ok(b) = serde_json::from_str::<Value>(&pt) {
                            match (
                                b.get("x").and_then(Value::as_str),
                                b.get("k").and_then(Value::as_str),
                            ) {
                                (Some(bx), Some(bk)) => {
                                    keys = Some((bx.to_string(), bk.to_string()));
                                    if let Some(t) = b.get("ticket").and_then(Value::as_str) {
                                        if !t.is_empty() {
                                            ticket = t.to_string();
                                        }
                                    }
                                    if let Some(n) = b.get("name").and_then(Value::as_str) {
                                        if !n.is_empty() {
                                            follower_name = n.to_string();
                                        }
                                    }
                                    trusted = true;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            // CHAT-ONLY: the peer scanned our QR to CHAT (chat_from_link), not follow. Bootstrap the
            // DM contact so we can chat both ways, but do NOT record a follower or notify "started
            // following you". A blocked DID is still rejected.
            if ev.payload.get("chat_only").and_then(Value::as_bool).unwrap_or(false) {
                if is_blocked_follower(&follower).await {
                    return false;
                }
                if keys.is_some() {
                    bootstrap_dm(&follower, &follower_name, &keys, &ticket).await;
                    dms::unhide_chat(&follower).await;
                    // CHAT-CAPABILITY: the peer used our CHAT link → permit a private chat both ways.
                    dms::enable_chat(&follower).await;
                    dms::rejoin_contact_topics(&follower).await;
                    return true;
                }
                return false;
            }
            record_follower(&follower, &ticket, &follower_name, &keys, ev.ts, trusted).await
        }
        _ => false,
    }
}

/// Record an incoming follow — shared by the (stripped) public `hey-social.follow`
/// feed event AND the SEALED follow announce (F-FOLLOW-ANNOUNCE). `follower` is
/// the VERIFIED signer/DM-sender DID in BOTH cases (feed events are Ed25519-signed;
/// sealed DMs carry the ratchet-bound `sender_did`), so neither path trusts a
/// self-asserted identity. Records the follower (+ pending classification +
/// flood/cap guards) and, for a SOLICITED follow that shared keys, bootstraps the
/// DM contact. Returns true if anything changed (drives the auto-refresh + notif).
async fn record_follower(
    follower: &str,
    ticket: &str,
    follower_name: &str,
    keys: &Option<(String, String)>,
    ts: i64,
    // TRUSTED = the follow arrived over an AUTHENTICATED+SEALED channel that proves
    // the sender held OUR pubkeys (a decrypted feed `enc` bundle, or a sealed DM
    // announce). That is an invited pairing — treat it as solicited so a single QR
    // scan materializes the DM contact (the pre-5b8e65d behavior), bypassing the
    // unsolicited-follow flood gate (still bounded by MAX_AUTO_CONTACTS).
    trusted: bool,
) -> bool {
    // A follower we explicitly removed/blocked may not silently re-appear.
    if is_blocked_follower(follower).await {
        log::info!(
            "ignoring follow from blocked {}",
            follower.chars().take(18).collect::<String>()
        );
        return false;
    }
    // RELATIONSHIP CLASSIFICATION (follow-flood gate):
    //  • reciprocated = I already follow them (a wanted, mutual follow)
    //  • already_recorded = they're already in followers.json (re-pair)
    //  • known_contact = an Active DM contact already exists
    // Any of these means an established relationship → bootstrap/refresh the DM
    // contact (idempotent reconcile, fixes the per-pair queue), exactly as before.
    // A BRAND-NEW, non-reciprocated follow is "unsolicited": it is RATE-LIMITED
    // (per-sender token bucket) and only RECORDED (pending) — we do NOT
    // auto-materialize ratchet/contact/queue state. follow_back() / start_chat()
    // read the pending record's keys and bootstrap on demand, so the legit
    // follow-back UX still opens a chat.
    let reciprocated = i_follow(follower).await;
    let known_contact = dms::find_contact(follower).await.is_some();
    // PRIVATE-ACCOUNT FOLLOWS: capture BOTH "is recorded at all" (for the flood-gate bypass) and
    // "is already an ACCEPTED (non-pending) follower" (the only case we auto-grant — a re-pair).
    let (already_recorded, already_accepted) = {
        let _g = storage_lock().lock().await;
        let list = shared_read_json("hey-social/followers.json")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let rec = list
            .iter()
            .find(|e| e.get("did").and_then(Value::as_str) == Some(follower))
            .cloned();
        let accepted = rec
            .as_ref()
            .map(|e| e.get("pending").and_then(Value::as_bool) != Some(true))
            .unwrap_or(false);
        (rec.is_some(), accepted)
    };
    let existing_relationship = reciprocated || already_recorded || known_contact;
    // PRIVATE-ACCOUNT: EVERY new follow is a REQUEST awaiting explicit Accept — it gets NO feed key
    // and NO bootstrapped contact until the user accepts (accept_follower). Only an already-accepted
    // follower re-pairing auto-grants (idempotent self-heal). So trusted / reciprocated / known new
    // follows are recorded PENDING, never silently granted feed access.
    let mut solicited = already_accepted;
    // F-DOS-LEAKED-LINK: a NEW invited pairing (trusted, no prior relationship) consumes a slot
    // from the GLOBAL trusted-bootstrap bucket. Over the cap, demote to UNSOLICITED so it records
    // PENDING instead of materializing a ratchet contact — follow_back/start_chat still bootstrap on
    // demand, so a legit scan is never lost; only an automated leaked-link flood is throttled.
    if trusted && !existing_relationship && !trusted_bootstrap_bucket_take() {
        log::warn!("trusted-bootstrap bucket exhausted; recording invited follow as pending");
        crate::guard::audit("follow.trusted_throttle", json!({ "did": follower }));
        solicited = false;
    }
    // Drop UNSOLICITED follows that exceed the per-sender burst budget, BEFORE
    // writing any state. A reciprocated/known follow bypasses the bucket (wanted).
    if !solicited && !follow_bucket_take(follower) {
        log::info!(
            "throttling unsolicited follow from {}",
            follower.chars().take(18).collect::<String>()
        );
        crate::guard::audit(
            "follow.throttle",
            json!({ "did": follower, "reason": "rate limit" }),
        );
        return false;
    }
    log::info!(
        "follow from {}: has_keys={} solicited={}",
        follower.chars().take(18).collect::<String>(),
        keys.is_some(),
        solicited
    );
    // Seed the peer-profile cache with the carried nickname so chat_contacts'
    // overlay shows the follower's real name immediately — before any DM and
    // before we ever subscribe to their feed for a profile broadcast. Only fills a
    // MISSING nickname; a signed profile broadcast stays authoritative.
    if !follower_name.is_empty() {
        seed_peer_nickname(follower, follower_name).await;
    }
    // `changed` = a brand-new follower record. `notify_pending` = a still-PENDING follower
    // re-requested (a fresh announce) that we never surfaced a notification for yet — so the
    // user is still alerted to act even if the very first request's notification was missed.
    let (changed, notify_pending) = {
        let _g = storage_lock().lock().await;
        let mut list = shared_read_json("hey-social/followers.json")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if let Some(existing) =
            list.iter_mut().find(|e| e.get("did").and_then(Value::as_str) == Some(follower))
        {
            // Already a recorded follower. Refresh keys/ticket if the stored record
            // lacks them (an earlier keyless follow) so a future boot reconcile can
            // bootstrap the DM contact from disk.
            let had_keys = existing.get("x").and_then(Value::as_str).is_some()
                && existing.get("k").and_then(Value::as_str).is_some();
            let mut dirty = false;
            if !had_keys && keys.is_some() {
                existing["ticket"] = json!(ticket);
                existing["x"] = json!(keys.as_ref().map(|(x, _)| x.clone()));
                existing["k"] = json!(keys.as_ref().map(|(_, k)| k.clone()));
                dirty = true;
            }
            // Re-request of a still-PENDING follower: notify when THIS announce is newer than the
            // one we last notified for. A genuine re-scan carries a fresh `ts`, so it re-notifies;
            // a replayed/duplicate announce (or re-processing the same one after a restart) repeats
            // an old `ts` and is skipped — same ts-gated dedup the reactions/comments use.
            let is_pending = existing.get("pending").and_then(Value::as_bool).unwrap_or(false);
            let last_notified_ts = existing.get("notified_ts").and_then(Value::as_i64).unwrap_or(0);
            let notify = !solicited && is_pending && ts > last_notified_ts;
            if notify {
                existing["notified_ts"] = json!(ts);
                dirty = true;
            }
            if dirty {
                let _ = shared_write_json("hey-social/followers.json", &json!(list)).await;
            }
            (false, notify)
        } else if list.len() >= MAX_FOLLOWERS {
            // Hard ceiling on the stored follower set: refuse to grow past it
            // (drops the spam tail; existing relationships are untouched).
            log::warn!("followers.json at MAX_FOLLOWERS ({MAX_FOLLOWERS}); dropping new follow");
            crate::guard::audit("follow.cap", json!({ "did": follower, "cap": MAX_FOLLOWERS }));
            (false, false)
        } else {
            // Record the follower. `pending` marks an unsolicited follow whose full
            // DM contact has NOT been materialized yet (backward-compatible:
            // absent/false on legacy + reciprocated records = a normal follower).
            // `notified_ts=ts` is set with the `changed` notification below, so re-processing
            // this same announce (restart / duplicate) repeats `ts` and won't re-notify.
            list.push(json!({
                "did": follower, "ticket": ticket, "ts": ts,
                "x": keys.as_ref().map(|(x, _)| x.clone()),
                "k": keys.as_ref().map(|(_, k)| k.clone()),
                "pending": !solicited,
                "notified_ts": ts,
            }));
            shared_write_json("hey-social/followers.json", &json!(list)).await.ok();
            (true, false)
        }
    };
    // Bootstrap the full DM contact (ratchet/queue) ONLY for a SOLICITED follow —
    // reciprocated, an existing follower (re-pair self-heal), or an already-active
    // contact. On a re-pair the contact can be missing (deleted / earlier keyless
    // follow); bootstrap_contact_from_keys is idempotent, so re-running it fixes
    // the per-pair queue. An UNSOLICITED follow is left PENDING — no
    // ratchet/contact/queue is created until the follow is mutual/accepted
    // (follow_back / start_chat), mirroring the group pending/consent model.
    let did_bootstrap = if solicited && keys.is_some() {
        bootstrap_dm(follower, follower_name, keys, ticket).await;
        // ISOLATION: this is the FEED re-pair path (a follow re-announce; a CHAT re-pair arrives as
        // chat_only and keeps unhide). Re-mesh the queues, but don't pop an empty chat row for a
        // feed action — hide until a real message. An active chat (last_ts>0) stays visible, and a
        // chat-link re-scan still resurrects via the chat_only branch.
        dms::hide_chat_if_empty(follower).await;
        dms::rejoin_contact_topics(follower).await;
        // PRIVATE FEED: hand this accepted follower my current feed key so they can open my sealed
        // posts. Self-gating: a no-op while the contact is verify-gated (never seals the key to
        // unverified/substitutable keys) — retried on chat-open once the user verifies them.
        send_feed_key_to(follower).await;
        // R6-TRUST-ELEVATION: do NOT clear the first-send verify gate from a feed-derived
        // follow. Decrypting the `enc` bundle proves ONLY that the sender knew our PUBLIC
        // friend-link keys — and that link is shareable/public, so anyone who has merely SEEN
        // it (not just someone we invited in person) can craft this exact "trusted" follow with
        // their OWN keys + a self-signed DID (the event signature binds the bundle to the
        // sender's own identity, which proves nothing about invitation). Clearing the gate would
        // let share_addresses auto-seal our wallet address card + call ticket to such a stranger
        // the moment we open the thread. So the contact still materializes (the chat works), but
        // the wallet card / call ticket stay GATED (needs_verify_before_send, set by bootstrap)
        // until an explicit user action — verify the safety number, "send anyway", or first send —
        // which is the real consent. Calls to an unverified contact surface the verify prompt.
        true
    } else {
        false
    };
    if changed || notify_pending {
        let name = peer_display_name(follower).await;
        // A solicited (mutual / accepted) follow is a done deal; an unsolicited one is a REQUEST
        // awaiting my Accept/Reject (it holds no feed key until I accept).
        let msg = if solicited { "started following you" } else { "wants to follow you" };
        log::info!(
            "follow-notif: enqueued '{msg}' for {} (changed={changed} notify_pending={notify_pending})",
            follower.chars().take(12).collect::<String>()
        );
        push_notif("follow", &name, msg, follower, follower);
    }
    changed || did_bootstrap
}

/// Dedup set for sealed follow announces already applied (per message id), so the
/// background poll doesn't re-record a follower every 2s tick.
fn follow_seen() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// F-FOLLOW-ANNOUNCE receive side: scan recent DM history for SEALED follow
/// announces (`FOLLOW_PREFIX`) and apply each ONCE via `record_follower`. The
/// announce arrived as a normal sealed+ratcheted DM, so `c.did` is the
/// cryptographically-authenticated sender — we never trust a payload-asserted
/// identity for WHO followed us (only their shared keys/ticket/name come from the
/// payload, exactly as the old feed event did). Runs on the same 2s background
/// loop as `poll_once`, so a follower is recorded even if the chat is never opened.
async fn process_sealed_follows() {
    let mut pending: Vec<(String, String, String, Option<(String, String)>, i64, bool)> = Vec::new();
    for c in dms::list_contacts().await {
        let conv = dms::read_conversation(&c.did).await;
        let arr = serde_json::to_value(&conv)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        for m in arr.iter().rev().take(16) {
            if m.get("mine").and_then(Value::as_bool).unwrap_or(false) {
                continue; // our own announce to them — not an inbound follow
            }
            let text = m.get("text").and_then(Value::as_str).unwrap_or("");
            // FEED-KEY ACK: a follower confirmed receipt of MY feed key → mark it delivered so
            // retry_pending_feed_keys stops re-sending. Authoritative sender = c.did (the ratchet-
            // bound contact, never a payload field); the ACK epoch must equal MY current epoch, so a
            // peer cannot ack on another's behalf or for a stale epoch.
            if let Some(b64) = text.strip_prefix(FEED_KEY_ACK_PREFIX) {
                if let Ok(bytes) = B64U.decode(b64.trim()) {
                    if let Ok(p) = serde_json::from_slice::<Value>(&bytes) {
                        let my_epoch = my_feed_epoch().await as u64;
                        if p.get("epoch").and_then(Value::as_u64) == Some(my_epoch) {
                            let _g = storage_lock().lock().await;
                            let mut sent = shared_read_json("hey-social/feed-key-sent.json").await.ok().flatten().unwrap_or_else(|| json!({}));
                            // Only write when it actually flips false→true (avoid a redundant write
                            // every poll tick while the ACK sits in the recent-message window).
                            let needs_flip = sent.get(c.did.as_str()).is_some_and(|r| {
                                r.get("epoch").and_then(Value::as_u64) == Some(my_epoch)
                                    && r.get("acked").and_then(Value::as_bool) != Some(true)
                            });
                            if needs_flip {
                                if let Some(rec) = sent.get_mut(c.did.as_str()) {
                                    rec["acked"] = json!(true);
                                }
                                let _ = shared_write_json("hey-social/feed-key-sent.json", &sent).await;
                            }
                        }
                    }
                }
                continue;
            }
            // PRIVATE FEED: an author I follow handed me their current feed key over this sealed DM.
            // The VERIFIED sender (c.did) is the AUTHORITATIVE author — never the payload `author`
            // field — so a contact can't poison my key cache for somebody else's feed.
            if let Some(b64) = text.strip_prefix(FEED_KEY_PREFIX) {
                let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
                let fid = format!(
                    "fk:{}",
                    m.get("id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{}:{}", c.did, ts))
                );
                {
                    let mut seen = crate::lock_safe(follow_seen());
                    if seen.contains(&fid) {
                        continue;
                    }
                    seen.insert(fid);
                    if seen.len() > 2000 {
                        seen.clear();
                    }
                }
                if let Ok(bytes) = B64U.decode(b64.trim()) {
                    if let Ok(p) = serde_json::from_slice::<Value>(&bytes) {
                        if let (Some(epoch), Some(k)) =
                            (p.get("epoch").and_then(Value::as_u64), p.get("key").and_then(Value::as_str))
                        {
                            cache_feed_key(&c.did, epoch as u32, k).await;
                            // FOLLOW-REQUEST: receiving the author's feed key == they ACCEPTED my
                            // follow → flip my "Requested" entry to "Following".
                            flip_following_pending_off(&c.did).await;
                            // ACK back so the author stops retrying (flips their record to delivered).
                            // At-most-once per received key — we're inside the follow_seen fid dedup.
                            let ack = json!({ "epoch": epoch });
                            let _ = chat_send(
                                &c.did,
                                &format!("{FEED_KEY_ACK_PREFIX}{}", B64U.encode(ack.to_string().as_bytes())),
                            )
                            .await;
                        }
                    }
                }
                continue;
            }
            // BLOCK SIGNAL (UI-only): the VERIFIED signer (c.did) BLOCKED me → record them in
            // blocked-by-peer.json so my UI shows "you've been blocked" + disables the composer for
            // this chat. Pure courtesy from the blocker; real enforcement is their inbound-drop.
            // Deduped (blk:) so a re-delivered DM doesn't thrash the list every poll.
            if text.starts_with(BLOCK_PREFIX) {
                let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
                let fid = format!(
                    "blk:{}",
                    m.get("id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{}:{}", c.did, ts))
                );
                {
                    let mut seen = crate::lock_safe(follow_seen());
                    if seen.contains(&fid) {
                        continue;
                    }
                    seen.insert(fid);
                    if seen.len() > 2000 {
                        seen.clear();
                    }
                }
                {
                    let _g = storage_lock().lock().await;
                    let mut list = shared_read_json("hey-social/blocked-by-peer.json")
                        .await
                        .ok()
                        .flatten()
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    // delete-dedup so no dupes accumulate across re-deliveries.
                    list.retain(|b| b.as_str() != Some(c.did.as_str()));
                    list.push(json!(c.did));
                    let _ = shared_write_json("hey-social/blocked-by-peer.json", &json!(list)).await;
                }
                continue;
            }
            // UNBLOCK SIGNAL (UI-only): the VERIFIED signer (c.did) UNBLOCKED me → drop them from
            // blocked-by-peer.json so my UI re-enables the composer. Deduped (unblk:).
            if text.starts_with(UNBLOCK_PREFIX) {
                let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
                let fid = format!(
                    "unblk:{}",
                    m.get("id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{}:{}", c.did, ts))
                );
                {
                    let mut seen = crate::lock_safe(follow_seen());
                    if seen.contains(&fid) {
                        continue;
                    }
                    seen.insert(fid);
                    if seen.len() > 2000 {
                        seen.clear();
                    }
                }
                {
                    let _g = storage_lock().lock().await;
                    let mut list = shared_read_json("hey-social/blocked-by-peer.json")
                        .await
                        .ok()
                        .flatten()
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    list.retain(|b| b.as_str() != Some(c.did.as_str()));
                    let _ = shared_write_json("hey-social/blocked-by-peer.json", &json!(list)).await;
                }
                continue;
            }
            // UNFOLLOW: the VERIFIED signer (c.did) unfollowed me → drop them from my followers +
            // rekey remaining (forward secrecy), so their old feed key can't open future posts AND a
            // later re-follow arrives as a fresh "wants to follow you" request. Deduped so a
            // re-delivered unfollow DM doesn't repeatedly rekey every poll.
            if text.starts_with(UNFOLLOW_PREFIX) {
                let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
                let fid = format!(
                    "unf:{}",
                    m.get("id").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{}:{}", c.did, ts))
                );
                {
                    let mut seen = crate::lock_safe(follow_seen());
                    if seen.contains(&fid) {
                        continue;
                    }
                    seen.insert(fid);
                    if seen.len() > 2000 {
                        seen.clear();
                    }
                }
                let _ = remove_follower(&c.did).await;
                continue;
            }
            let Some(b64) = text.strip_prefix(FOLLOW_PREFIX) else { continue };
            let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
            let id = m
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}:{}", c.did, ts));
            // O(1) dedup BEFORE the heavier record_follower work.
            {
                let mut seen = crate::lock_safe(follow_seen());
                if seen.contains(&id) {
                    continue;
                }
                seen.insert(id.clone());
                if seen.len() > 2000 {
                    seen.clear();
                }
            }
            let Ok(bytes) = B64U.decode(b64.trim()) else { continue };
            let Ok(p) = serde_json::from_slice::<Value>(&bytes) else { continue };
            let ticket = p.get("ticket").and_then(Value::as_str).unwrap_or("").to_string();
            let name = p.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let keys = match (p.get("x").and_then(Value::as_str), p.get("k").and_then(Value::as_str)) {
                (Some(x), Some(k)) => Some((x.to_string(), k.to_string())),
                _ => None,
            };
            // CHAT-ONLY: a scanned chat invite's announce carries chat_only=true → pair a chat
            // contact, don't record a follower.
            let chat_only = p.get("chat_only").and_then(Value::as_bool).unwrap_or(false);
            // VERIFIED follower = the DM sender (c.did), NOT a payload field.
            pending.push((c.did.clone(), ticket, name, keys, ts, chat_only));
        }
    }
    for (follower, ticket, name, keys, ts, chat_only) in pending {
        // A sealed follow-announce arrived over the authenticated ratchet (c.did is
        // the cryptographically-bound sender) → trusted invited pairing.
        if chat_only {
            // CHAT-ONLY: bootstrap the DM contact (chat both ways) but DON'T record a follower or
            // notify "started following you". Blocked DIDs stay rejected.
            if !is_blocked_follower(&follower).await && keys.is_some() {
                bootstrap_dm(&follower, &name, &keys, &ticket).await;
                dms::unhide_chat(&follower).await;
                // CHAT-CAPABILITY: a chat_only announce is explicit chat consent → permit chat.
                dms::enable_chat(&follower).await;
                dms::rejoin_contact_topics(&follower).await;
            }
        } else {
            record_follower(&follower, &ticket, &name, &keys, ts, true).await;
        }
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
    // Soft-deleted chats: the user wiped local history + hid the row, but we KEEP the
    // contact + queues so a new message re-opens it. Hide the row from the list until
    // a message arrives (dms::touch_contact_message un-hides) or it's explicitly re-added.
    let hidden = dms::hidden_chat_set().await;
    let mut out: Vec<Value> = Vec::new();
    // DEDUP by DID: messy re-pair/scan cycles can leave two contacts for the same DID. Emit each
    // DID once (the UI keys chat rows by DID, so a duplicate would otherwise crash the list on draw).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in dms::list_contacts().await {
        if hidden.contains(&c.did) {
            continue;
        }
        if !seen.insert(c.did.clone()) {
            continue; // already emitted this DID
        }
        // BLOCK enforcement (read-side, defense-in-depth): a blocked DID's chat is
        // hidden here as well as in the Kotlin layer, so a message that still
        // arrived over the carrier (the receiver is hey-core's and we don't patch
        // it) can't surface a conversation for someone you've blocked.
        if is_blocked_follower(&c.did).await {
            continue;
        }
        let mut v = serde_json::to_value(&c).unwrap_or_else(|_| json!({}));
        // Overlay the contact's CURRENT nickname + avatar from their cached profile, so a profile
        // edit shows in the chat list/header (the invite-time name stays as the fallback).
        let prof = raw_profile(&c.did, &me).await;
        // CONVERGE the two name systems: the live DM "sn"/profile_name path writes
        // DmContact.name (hey-core), while this overlay reads the feed-profile cache.
        // Don't let a STALE cache hide a fresher live name: when the contact carries
        // a real (non-generated) name AND the cached profile is OLDER than the last
        // DM we received (cache ts < last_ts), prefer the live DmContact.name. A
        // cache at least as fresh (a recent profile broadcast) still wins.
        if let Some(n) = prof.get("nickname").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            let cache_ts = prof.get("ts").and_then(Value::as_i64).unwrap_or(0);
            let live_is_real = !dms::is_generated_label(&c.name);
            if !(live_is_real && cache_ts < c.last_ts) {
                v["name"] = json!(n);
            }
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

/// Admin-facing detail for one group: roster + each member's live nickname +
/// admin/creator flags, plus whether *I* may run owner/admin ops. Mirrors the
/// JNI contract the Kotlin group-admin sheet consumes.
pub async fn group_info(gid: &str) -> Value {
    ensure_session().await.ok();
    let me = whoami_did().await.unwrap_or_default();
    let Some(g) = dms::list_groups().await.into_iter().find(|g| g.id == gid) else {
        return json!({ "error": "no such group" });
    };
    let am_admin = me == g.created_by || g.admins.iter().any(|a| *a == me);
    let mut members: Vec<Value> = Vec::new();
    for m in &g.members {
        // Overlay the member's CURRENT nickname from their cached profile (same
        // source chat_contacts uses), falling back to the roster-time name.
        let prof = raw_profile(&m.did, &me).await;
        let name = prof
            .get("nickname")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| m.name.clone());
        members.push(json!({
            "did": m.did,
            "name": name,
            "admin": g.admins.iter().any(|a| *a == m.did),
            "isCreator": m.did == g.created_by,
        }));
    }
    json!({
        "id": g.id,
        "name": g.name,
        "avatar": g.avatar_cid,
        "createdBy": g.created_by,
        "amAdmin": am_admin,
        "closed": g.closed,
        "members": members,
    })
}

/// Add contacts to a group (owner/admin only — enforced by the engine).
/// `dids_json` = JSON array of contact DIDs. Returns {ok:true} or an error.
pub async fn chat_group_add_members(gid: &str, dids_json: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    let dids: Vec<String> =
        serde_json::from_str(dids_json).map_err(|e| format!("parse dids: {e}"))?;
    dms::add_group_members(gid, dids).await.map(|_| json!({ "ok": true }))
}

/// Remove a member from a group (owner/admin only — enforced by the engine).
/// Returns {ok:true} or an error.
pub async fn chat_group_remove_member(gid: &str, did: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    dms::remove_group_member(gid, did).await.map(|_| json!({ "ok": true }))
}

/// Promote a member to admin (owner/admin only — enforced by the engine).
/// Returns {ok:true} or an error.
pub async fn chat_group_add_admin(gid: &str, member_did: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    dms::add_group_admin(gid, member_did).await.map(|_| json!({ "ok": true }))
}

/// Set the group picture (owner/admin only — enforced by the engine). `picture` is
/// the CID/ref of a pre-uploaded image (same convention as a profile avatar); pass
/// "" to clear it. Returns {ok:true} or an error.
pub async fn chat_group_set_picture(gid: &str, picture: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    dms::set_group_picture(gid, picture).await.map(|_| json!({ "ok": true }))
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
        if text.starts_with(CALL_PREFIX)
            || text.starts_with(DEL_PREFIX)
            || text.starts_with(EDIT_PREFIX)
            || text.starts_with(FOLLOW_PREFIX)
            || text.starts_with(UNFOLLOW_PREFIX)
            || text.starts_with(BLOCK_PREFIX)
            || text.starts_with(UNBLOCK_PREFIX)
            || text.starts_with(FEED_KEY_PREFIX)
            || text.starts_with(FEED_KEY_ACK_PREFIX)
        {
            // Hidden control messages (call / delete / edit / sealed follow announce / feed key)
            // — handled elsewhere (process_sealed_follows for FOLLOW + FEED_KEY), never shown.
            continue;
        }
        if deleted.contains(&(id, mine)) {
            continue; // deleted by its sender
        }
        out.push(apply_edit(m, &edits));
    }
    let _ = share_addresses(did).await; // once per peer; opening the chat bootstraps it
    // PRIVATE FEED: retry feed-key delivery on chat-open — self-gates, so it fires only once the
    // contact is a follower AND no longer verify-gated (de-duped per epoch, so this is cheap).
    let _ = send_feed_key_to(did).await;
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
/// True iff a private chat with `did` is permitted (chat was explicitly established via a chat
/// QR/invite, or a real conversation already exists). FALSE for a follow-only contact — the UI uses
/// this to hide the "Message" affordance, and dms::send_message refuses the send regardless, so
/// following someone can never open a chat.
pub async fn can_chat(did: &str) -> bool {
    dms::is_chat_enabled(did).await
}

pub async fn chat_send(did: &str, text: &str) -> Result<Value, String> {
    ensure_session().await.ok();
    // A gated send (needs_verify / key-CHANGED MITM alarm) MUST surface as a
    // visible Err — never a silent "success" — so the UI shows the failure and the
    // key-change alarm stays fatal. The message still survives because
    // send_message persists the local echo BEFORE the gate (Fix 1, dms.rs); the
    // echo renders on the next thread refresh even though send() reports failure.
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

/// Like [peer_ticket] but ONLY returns a ticket the contact SELF-ASSERTED (proved their own
/// EndpointId) — never an owner/discovery-poisoned one. Used for CALL + verse media dialing so a
/// malicious group owner can't bootstrap a victim-side discovery contact pointing at the attacker's
/// endpoint and silently redirect the victim's live mic/camera/presence. Empty ⇒ fail closed (the
/// Kotlin call paths skip the dial on an empty ticket). Mirrors the group-call hardening.
pub async fn peer_ticket_self_asserted(did: &str) -> String {
    dms::peer_ticket_self_asserted(did).await.unwrap_or_default()
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

/// Send a 1:1 message QUOTING another (tap-to-reply). The quote (id/author/snippet)
/// rides inside the sealed body and is stored on the message for both sides.
pub async fn chat_send_reply(
    did: &str,
    text: &str,
    reply_id: &str,
    reply_author: &str,
    reply_snippet: &str,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let reply = dms::ReplyRef {
        id: reply_id.to_string(),
        author: reply_author.to_string(),
        snippet: reply_snippet.to_string(),
    };
    dms::send_message_reply(did, text, reply)
        .await
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
}

/// Group counterpart of [chat_send_reply].
pub async fn chat_send_group_reply(
    gid: &str,
    text: &str,
    reply_id: &str,
    reply_author: &str,
    reply_snippet: &str,
) -> Result<Value, String> {
    ensure_session().await.ok();
    let reply = dms::ReplyRef {
        id: reply_id.to_string(),
        author: reply_author.to_string(),
        snippet: reply_snippet.to_string(),
    };
    dms::send_group_message_reply(gid, text, reply)
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
    dms::generate_invite(label, dms::IdentityMode::Regular, "").await
}

// (Incognito chat invite removed — only regular chat invites remain.)
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

// ── reassembler DoS caps (mirrors frag.rs MAX_PARTIALS) ──────────────────────
// Bound the distinct-CID reassembler so a flood of forged `hey-social.blob`
// events on a followed topic can't pin unbounded RAM between TTL sweeps. Capped
// by COUNT (concurrent reassemblies) AND by total buffered BYTES; the oldest
// entry is evicted when either ceiling is hit. Legit transfers reassemble one
// (or a few) media at a time, so these ceilings never break a real transfer.
const MAX_PENDING_CIDS: usize = 16; // mirror frag.rs MAX_PARTIALS
const MAX_PENDING_BYTES: usize = 24 * 1024 * 1024; // ~24 MB live reassembly budget

/// Sum the bytes currently buffered across all in-flight reassemblies.
fn pending_blob_bytes(p: &HashMap<String, (i64, Vec<Option<Vec<u8>>>)>) -> usize {
    p.values()
        .map(|(_, slots)| slots.iter().flatten().map(Vec::len).sum::<usize>())
        .sum()
}

/// Evict the oldest reassembly entry (by inserted_ms); used to keep the
/// reassembler under MAX_PENDING_CIDS / MAX_PENDING_BYTES. Returns true if one
/// was dropped.
fn evict_oldest_pending(p: &mut HashMap<String, (i64, Vec<Option<Vec<u8>>>)>) -> bool {
    if let Some(oldest) = p.iter().min_by_key(|(_, (t, _))| *t).map(|(k, _)| k.clone()) {
        p.remove(&oldest);
        true
    } else {
        false
    }
}

// ── per-sender per-event-kind cooldown (sync_req / blob_req amplification) ────
// A signed peer can spam `sync_req`/`blob_req` to force us to re-broadcast our
// whole feed or re-chunk media over and over. Gate each (sender, kind) pair to
// at most one honored request per COOLDOWN window; repeats inside the window are
// dropped (the legitimate one already triggered the response). In-memory only —
// a process restart just reopens the window, which is harmless.
const REQ_COOLDOWN_MS: i64 = 20_000; // 20s per (sender, kind)
const REQ_COOLDOWN_CAP: usize = 4096; // bound the cooldown map itself

/// (sender_did, kind) -> last_honored_ms. Returns true if this request should be
/// honored now (and records the timestamp); false if it's inside the cooldown.
fn req_cooldown_ok(sender: &str, kind: &str) -> bool {
    static M: OnceLock<Mutex<HashMap<(String, String), i64>>> = OnceLock::new();
    let m = M.get_or_init(|| Mutex::new(HashMap::new()));
    let now = hey_core::plat::now_ms();
    let mut g = crate::lock_safe(m);
    let key = (sender.to_string(), kind.to_string());
    if let Some(&last) = g.get(&key) {
        if now - last < REQ_COOLDOWN_MS {
            return false;
        }
    }
    // Coarse cap: if the map balloons (many distinct senders), clear it — the
    // worst case is one extra honored request per sender after the wipe.
    if g.len() >= REQ_COOLDOWN_CAP && !g.contains_key(&key) {
        g.clear();
    }
    g.insert(key, now);
    true
}

// ── follow-flood gate (unsolicited public-topic follow ingestion) ────────────
// The public per-author feed topic lets ANY signer publish `hey-social.follow`,
// which would otherwise materialize followers.json + a DmContact + ratchet/queue
// state with no prior relationship — an unbounded resource + spam vector. Gate
// inbound follow INGESTION with a per-sender token bucket, and cap the stored
// follower set. A follow is RECORDED (pending) but a full Active DM contact is
// only bootstrapped once the relationship is mutual (we follow them too) —
// mirroring the group pending/consent model. The legit UX is preserved: a follow
// YOU initiated (follow()/follow_back()) bootstraps immediately on the send side,
// and a reciprocated follow bootstraps here on ingest.
const MAX_FOLLOWERS: usize = 5000; // hard ceiling on followers.json
const MAX_AUTO_CONTACTS: usize = 5000; // hard ceiling on auto-bootstrapped contacts
const FOLLOW_BUCKET_CAP: f64 = 5.0; // burst of unsolicited follows per sender
const FOLLOW_REFILL_PER_MS: f64 = 5.0 / 3_600_000.0; // ~5/hour steady refill

/// Per-sender token bucket for inbound follow ingestion. Returns true if a token
/// was available (consume it). A follow YOU already follow back bypasses the
/// bucket at the call site (it's a wanted, reciprocated follow).
fn follow_bucket_take(sender: &str) -> bool {
    // sender -> (tokens, last_refill_ms)
    static B: OnceLock<Mutex<HashMap<String, (f64, i64)>>> = OnceLock::new();
    let b = B.get_or_init(|| Mutex::new(HashMap::new()));
    let now = hey_core::plat::now_ms();
    let mut g = crate::lock_safe(b);
    if g.len() >= REQ_COOLDOWN_CAP && !g.contains_key(sender) {
        g.clear();
    }
    let e = g.entry(sender.to_string()).or_insert((FOLLOW_BUCKET_CAP, now));
    let elapsed = (now - e.1).max(0) as f64;
    e.0 = (e.0 + elapsed * FOLLOW_REFILL_PER_MS).min(FOLLOW_BUCKET_CAP);
    e.1 = now;
    if e.0 >= 1.0 {
        e.0 -= 1.0;
        true
    } else {
        false
    }
}

const TRUSTED_BOOTSTRAP_CAP: f64 = 50.0; // burst of NEW invited (friend-link) auto-bootstraps
const TRUSTED_BOOTSTRAP_REFILL_PER_MS: f64 = 50.0 / 3_600_000.0; // ~50/hour steady refill

/// GLOBAL token bucket for NEW invited (sealed friend-link) auto-bootstraps. A friend link / QR is
/// SHAREABLE — anyone who has merely seen it knows our public keys and can mint a fresh DID + craft
/// a "trusted" sealed follow, so the per-SENDER bucket can't bound a multi-DID flood. This global
/// bucket caps the instant materialization of ratchet contacts from leaked-link follows (the prior
/// bound was only MAX_AUTO_CONTACTS=5000, an instant resource-DoS). A real user adds far fewer than
/// 50 contacts/hour; over the cap, the follow is RECORDED PENDING (follow_back/start_chat still
/// bootstrap on demand) — never dropped. An EXISTING relationship bypasses this at the call site.
fn trusted_bootstrap_bucket_take() -> bool {
    static B: OnceLock<Mutex<(f64, i64)>> = OnceLock::new();
    let b = B.get_or_init(|| Mutex::new((TRUSTED_BOOTSTRAP_CAP, 0)));
    let now = hey_core::plat::now_ms();
    let mut g = crate::lock_safe(b);
    if g.1 == 0 {
        g.1 = now;
    }
    let elapsed = (now - g.1).max(0) as f64;
    g.0 = (g.0 + elapsed * TRUSTED_BOOTSTRAP_REFILL_PER_MS).min(TRUSTED_BOOTSTRAP_CAP);
    g.1 = now;
    if g.0 >= 1.0 {
        g.0 -= 1.0;
        true
    } else {
        false
    }
}

/// True if I currently follow `did` (so an inbound follow from them is a wanted,
/// reciprocated follow → bootstrap a full DM contact). Read-fallback safe.
async fn i_follow(did: &str) -> bool {
    shared_read_json("hey-social/following.json")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|e| e.get("did").and_then(Value::as_str) == Some(did))
}

/// F-CONTENT-CID-NOSCOPE: true ONLY if `cid` is referenced by my PUBLIC surface —
/// my profile avatar, or the media / author_avatar of one of MY OWN posts (those I
/// authored and broadcast on my feed topic). A blob_req on my feed topic is served
/// only for such CIDs, so a removed group member can't make me re-serve a PRIVATE
/// group-avatar (or any other) CID I merely happen to hold. Bounds the scan to my
/// own posts (`author == me`) read off the local feed index.
async fn is_public_media_cid(cid: &str, me: &str) -> bool {
    if cid.is_empty() {
        return false;
    }
    // My published profile avatar is public.
    if my_profile().await.get("avatar").and_then(Value::as_str) == Some(cid) {
        return true;
    }
    // Media (and the denormalized author_avatar) of MY OWN posts are public.
    let idx: Vec<Value> = shared_read_json(FEED_INDEX)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    for id in idx.iter().filter_map(Value::as_str) {
        let Ok(Some(p)) = shared_read_json(&format!("hey-social/posts/{id}.json")).await else {
            continue;
        };
        if p.get("author").and_then(Value::as_str) != Some(me) {
            continue; // only vouch for CIDs in posts I authored
        }
        if p.get("author_avatar").and_then(Value::as_str) == Some(cid) {
            return true;
        }
        if let Some(arr) = p.get("media").and_then(|v| v.as_array()) {
            if arr.iter().any(|m| m.get("cid").and_then(Value::as_str) == Some(cid)) {
                return true;
            }
        }
    }
    false
}

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

/// On-demand media pull for the feed UI — fetch `cid` from `author`'s topic if not already
/// local. Fire-and-forget (just kicks the blob_req + dedupes); the UI then polls media_ready.
/// Called when a media tile scrolls into view, so a backfill never downloads everything at once.
pub async fn ensure_media(cid: &str, author: &str) {
    ensure_blob(cid, author).await;
}

/// True once a feed media blob is downloaded locally — the UI swaps its loading card for the image.
pub async fn media_ready(cid: &str) -> bool {
    !cid.is_empty() && content::get_bytes(cid, None).await.is_ok()
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
    // Re-broadcast MY current public profile (nickname/bio/avatar — NO addresses,
    // see broadcast_profile F-04) on my feed
    // topic FIRST, so a brand-new follower's add-time backfill carries my live
    // name — closing the gap where a name changed BEFORE they subscribed (the
    // one-shot broadcast_profile at edit time never reached them). Same shape as
    // broadcast_profile; the freshness-gated profile handler de-dupes by ts.
    let p = my_profile().await;
    if p.is_object() {
        broadcast_profile(me, &p).await;
    }
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
                // PRIVATE FEED: re-seal each backfilled post under my CURRENT epoch, so a follower
                // only ever needs my current key (new followers read recent history; a removed
                // follower's old key opens nothing, even on re-broadcast).
                let wire = seal_post_outbound(&p).await;
                publish(&feed_topic(me), "hey-social.post", wire).await;
                sent += 1;
            }
        }
    }
}

#[cfg(test)]
mod follower_dedup_tests {
    use super::dedupe_followers;
    use serde_json::json;

    #[test]
    fn collapses_pending_and_accepted_to_accepted() {
        // The exact bug: an accepted record AND a leftover pending record for the same DID.
        let arr = vec![
            json!({ "did": "did:key:zX", "pending": false, "ts": 100, "notified_ts": 100, "x": "xx", "k": "kk", "ticket": "tk" }),
            json!({ "did": "did:key:zX", "pending": true,  "ts": 90,  "notified_ts": 90 }),
        ];
        let (out, changed) = dedupe_followers(&arr);
        assert!(changed);
        assert_eq!(out.len(), 1);
        // accepted wins, newest ts kept, keys preserved from the copy that had them.
        assert_eq!(out[0]["pending"], json!(false));
        assert_eq!(out[0]["ts"], json!(100));
        assert_eq!(out[0]["x"], json!("xx"));
    }

    #[test]
    fn both_pending_stays_pending_keeps_latest_notified() {
        let arr = vec![
            json!({ "did": "did:key:zY", "pending": true, "ts": 10, "notified_ts": 10 }),
            json!({ "did": "did:key:zY", "pending": true, "ts": 20, "notified_ts": 25 }),
        ];
        let (out, changed) = dedupe_followers(&arr);
        assert!(changed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["pending"], json!(true));
        assert_eq!(out[0]["ts"], json!(20));
        assert_eq!(out[0]["notified_ts"], json!(25));
    }

    #[test]
    fn unique_list_unchanged_and_order_preserved() {
        let arr = vec![
            json!({ "did": "did:key:zA", "pending": true }),
            json!({ "did": "did:key:zB", "pending": false }),
        ];
        let (out, changed) = dedupe_followers(&arr);
        assert!(!changed);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["did"], json!("did:key:zA"));
        assert_eq!(out[1]["did"], json!("did:key:zB"));
    }

    #[test]
    fn drops_malformed_entry_without_did() {
        let arr = vec![json!({ "pending": true }), json!({ "did": "did:key:zC" })];
        let (out, changed) = dedupe_followers(&arr);
        assert!(changed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["did"], json!("did:key:zC"));
    }
}

#[cfg(test)]
mod hyper_link_tests {
    use super::*;

    // Deterministic test identity: seed -> Ed25519 pubkey -> did:key.
    fn test_identity() -> ([u8; 32], String) {
        let seed = [7u8; 32];
        let pk = hey_core::identity::public_key_from_seed(&seed);
        (seed, hey_core::identity::public_key_to_did_key(&pk))
    }

    fn dummy_keys() -> (String, String) {
        let x = B64S.encode([3u8; 32]); // X25519 pub = 32 bytes
        let k = B64S.encode(vec![9u8; ML_KEM_PUB_LEN]); // ML-KEM-768 pub = 1184 bytes
        (x, k)
    }

    #[test]
    fn follow_link_round_trips_and_verifies() {
        let (seed, did) = test_identity();
        let (x, k) = dummy_keys();
        // A real compact ticket is base32; use a base32 string so the raw-ticket path is exercised.
        let ticket = data_encoding::BASE32_NOPAD.encode(b"{\"node\":\"abc\"}");
        let link = encode_hyper_link(HYPER_MAGIC_FOLLOW, "hyper:follow:", &did, &x, &k, &ticket, &[], &seed).unwrap();
        assert!(link.starts_with("hyper:follow:"));
        let body = link.strip_prefix("hyper:follow:").unwrap();
        let (d, t, dx, dk, extra, verified) = decode_hyper_link(body, HYPER_MAGIC_FOLLOW, 0).unwrap();
        assert!(verified);
        assert_eq!(d, did);
        assert_eq!(t, ticket); // ticket survives the raw-decode/re-encode round trip
        assert_eq!(dx, x);
        assert_eq!(dk, k);
        assert!(extra.is_empty());
    }

    #[test]
    fn parse_follow_returns_follow_intent_and_keys() {
        let (seed, did) = test_identity();
        let (x, k) = dummy_keys();
        let link = encode_hyper_link(HYPER_MAGIC_FOLLOW, "hyper:follow:", &did, &x, &k, "TICKET", &[], &seed).unwrap();
        let (d, _t, keys, verified, intent) = parse_follow(&link).unwrap();
        assert_eq!(d, did);
        assert!(verified);
        assert_eq!(intent, LinkIntent::Follow);
        assert_eq!(keys, Some((x, k)));
    }

    #[test]
    fn tampered_key_is_rejected() {
        let (seed, did) = test_identity();
        let (x, k) = dummy_keys();
        let link = encode_hyper_link(HYPER_MAGIC_FOLLOW, "hyper:follow:", &did, &x, &k, "T", &[], &seed).unwrap();
        let body = link.strip_prefix("hyper:follow:").unwrap();
        let mut raw = B64U.decode(body).unwrap();
        raw[70] ^= 0xff; // flip a byte inside the ML-KEM key -> signature must fail
        let tampered = B64U.encode(&raw);
        assert!(decode_hyper_link(&tampered, HYPER_MAGIC_FOLLOW, 0).is_none());
    }

    #[test]
    fn wrong_magic_is_rejected() {
        let (seed, did) = test_identity();
        let (x, k) = dummy_keys();
        let link = encode_hyper_link(HYPER_MAGIC_FOLLOW, "hyper:follow:", &did, &x, &k, "T", &[], &seed).unwrap();
        let body = link.strip_prefix("hyper:follow:").unwrap();
        // Decoding a follow container as a chat container (magic 0xC1) must fail.
        assert!(decode_hyper_link(body, 0xC1, 0).is_none());
    }

    #[test]
    fn ticket_text_fallback_round_trips() {
        // A non-base32 ticket falls back to utf8 text storage (tag 1) and must round-trip exactly.
        let (seed, did) = test_identity();
        let (x, k) = dummy_keys();
        let ticket = "not-base32!@#"; // '!' '@' '#' are not in the base32 alphabet
        let link = encode_hyper_link(HYPER_MAGIC_FOLLOW, "hyper:follow:", &did, &x, &k, ticket, &[], &seed).unwrap();
        let body = link.strip_prefix("hyper:follow:").unwrap();
        let (_d, t, _dx, _dk, _e, _v) = decode_hyper_link(body, HYPER_MAGIC_FOLLOW, 0).unwrap();
        assert_eq!(t, ticket);
    }

    #[test]
    fn chat_link_intent_and_cross_magic_isolation() {
        let (seed, did) = test_identity();
        let (x, k) = dummy_keys();
        let link = encode_hyper_link(HYPER_MAGIC_CHAT, "hyper:chat:", &did, &x, &k, "T", &[], &seed).unwrap();
        assert!(link.starts_with("hyper:chat:"));
        let (d, _t, keys, verified, intent) = parse_follow(&link).unwrap();
        assert_eq!(d, did);
        assert!(verified);
        assert_eq!(intent, LinkIntent::Chat);
        assert_eq!(keys, Some((x, k)));
        // A chat container can NEVER be decoded as a follow (magic mismatch) — follow!=chat isolation.
        let body = link.strip_prefix("hyper:chat:").unwrap();
        assert!(decode_hyper_link(body, HYPER_MAGIC_FOLLOW, 0).is_none());
    }
}
