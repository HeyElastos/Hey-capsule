//! Hey Verse EPHEMERAL gossip presence lane — movement over RAW gossip.
//!
//! Movement is high-rate and PERISHABLE; it has no business on the DM lane
//! (Double-Ratchet seal + ratchet-state disk write + broker.json persist on
//! receive, all per tick — that flood is what crashed a walking phone). The
//! fast lane (`verse_rt`) carries movement over direct QUIC datagrams; THIS lane
//! is its gossip twin for when a direct datagram link isn't up yet: everyone in
//! a world joins ONE shared topic (`hey/verse/room/{world}`) and a single
//! broadcast reaches them all.
//!
//! AUTHENTICATED, not encrypted: presence is public (a shared world), so frames
//! are NOT ratchet-sealed — but EVERY frame is Ed25519-signed by the sender's
//! identity seed and verified on receive against the pubkey embedded in the
//! sender's did:key. A peer therefore cannot forge movement/chat attributed to
//! another DID (the audit's spoofing finding), and stale frames are rejected.
//!
//! Guarantees, by construction (see `Carrier::subscribe_ephemeral`):
//!   * no Double-Ratchet seal — raw `encode_wire` framing, signed not encrypted;
//!   * no `broker.json` write on receive — the receiver hands frames straight to
//!     an in-RAM inbox, it never touches the broker;
//!   * not persisted — the topic is never recorded in `broker.subscriptions`, so
//!     it is NOT re-joined on the next boot (nothing survives a restart);
//!   * received frames are deposited into the SAME inbox Godot already drains via
//!     the fast lane (`verse_rt::deposit`), so the game sees gossip and datagram
//!     movement through one unchanged path.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use iroh_gossip::api::GossipSender;

use crate::carrier::Carrier;

/// The live lane: the broadcast handle, the world topic it's bound to, and our
/// own did:key (stamped on outbound frames + used to drop our own echo).
struct Live {
    sender: Arc<GossipSender>,
    topic: String,
}

static LIVE: OnceLock<Mutex<Option<Live>>> = OnceLock::new();
/// Bumped on reset()/world-switch so the old receiver loop dies with the session.
static GEN: AtomicU64 = AtomicU64::new(0);

fn live() -> &'static Mutex<Option<Live>> {
    LIVE.get_or_init(|| Mutex::new(None))
}

/// Per-world presence namespace. Domain-separated from DM queues (`q/<id>`) and
/// feed topics (`hey-social/feed/...`) by its literal prefix.
fn verse_topic(world: &str) -> String {
    format!("hey/verse/room/{world}")
}

/// Canonical bytes signed over each frame — domain-separated, binding the
/// sender DID + world topic + timestamp + content so a frame cannot be replayed
/// into another world or re-attributed to another DID.
fn signing_bytes(sender_did: &str, topic: &str, ts: i64, content: &str) -> Vec<u8> {
    format!("hey/verse-gossip/1\u{0}{sender_did}\u{0}{topic}\u{0}{ts}\u{0}{content}").into_bytes()
}

/// Join (or switch to) the ephemeral movement topic for `world`, bootstrapping
/// with whatever contact tickets we hold for the currently-present peers. `me`
/// is our own did:key. Idempotent while already on the same world.
pub async fn join(carrier: Arc<Carrier>, world: String, me: String, bootstrap: Vec<String>) {
    let topic = verse_topic(&world);
    // Already on this exact topic? Gossip is a self-healing plumtree mesh, so a
    // re-seed isn't needed — keep the live lane.
    if let Some(cur) = crate::lock_safe(live()).as_ref() {
        if cur.topic == topic {
            return;
        }
    }
    // First join, or a world switch: bump GEN to kill any old receiver loop,
    // then subscribe fresh on the new topic.
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let me_echo = me;
    let topic_v = topic.clone();
    let sender = carrier
        .subscribe_ephemeral(
            &topic,
            &bootstrap,
            g,
            || GEN.load(Ordering::SeqCst),
            move |sender_did: String, content: String, ts: i64, sig: String| {
                // Drop our own echo (we're a member of our own topic) + empty senders.
                if sender_did.is_empty() || sender_did == me_echo {
                    return;
                }
                // Freshness: movement is perishable — reject stale/replayed frames.
                // Generous window so normal device clock skew never drops live frames;
                // movement replay is harmless anyway (the next real frame overwrites).
                let now = hey_core::plat::now_ms();
                if (now - ts).abs() > 120_000 {
                    return;
                }
                // Authenticity: verify the Ed25519 signature against the pubkey
                // EMBEDDED in the sender's did:key — no roster/pinning lookup, and
                // an attacker cannot forge a frame as another DID without its key.
                let pk = match hey_core::identity::did_key_to_public_key(&sender_did) {
                    Ok(pk) => pk,
                    Err(_) => return,
                };
                if !hey_core::identity::verify(
                    &signing_bytes(&sender_did, &topic_v, ts, &content),
                    &sig,
                    &pk,
                ) {
                    return; // unsigned / forged / wrong-world → drop
                }
                crate::verse_rt::deposit(sender_did, content);
            },
        )
        .await;
    if let Some(sender) = sender {
        *crate::lock_safe(live()) = Some(Live {
            sender: Arc::new(sender),
            topic,
        });
    }
}

/// Broadcast one movement payload (raw JSON) to the live world topic. Silent
/// no-op until a topic is joined. No seal, no ratchet, no disk.
pub async fn send_all(payload: String) {
    let (sender, topic) = match crate::lock_safe(live()).as_ref() {
        Some(cur) => (cur.sender.clone(), cur.topic.clone()),
        None => return,
    };
    // Sign every frame with our local Ed25519 identity so a peer cannot forge
    // movement/chat attributed to another DID. The wire `s` is our did:key (which
    // embeds the verify key); `t` is a fresh timestamp; `g` is the signature.
    let id = match crate::IDENTITY.get() {
        Some(id) => id,
        None => return, // identity not loaded yet — verse_rt still carries movement
    };
    let me = id.did_key().to_string();
    let ts = hey_core::plat::now_ms();
    let sig = hey_core::identity::sign(&signing_bytes(&me, &topic, ts, &payload), &id.seed());
    let bytes = crate::carrier::encode_wire(&payload, &me, ts, &sig);
    let _ = sender.broadcast(bytes).await;
}

/// Tear down the lane (session emptied / world closed). Bumping GEN kills the
/// receiver loop; dropping LIVE releases the gossip subscription.
pub fn reset() {
    GEN.fetch_add(1, Ordering::SeqCst);
    *crate::lock_safe(live()) = None;
}

/// True if the gossip presence lane is currently joined to a world topic.
pub fn connected() -> bool {
    crate::lock_safe(live()).is_some()
}
