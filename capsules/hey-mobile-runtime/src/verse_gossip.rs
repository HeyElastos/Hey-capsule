//! Hey Verse EPHEMERAL gossip presence lane — movement over RAW gossip.
//!
//! Movement is high-rate and PERISHABLE; it has no business on the DM lane
//! (Double-Ratchet seal + ratchet-state disk write + broker.json persist on
//! receive, all per tick — that flood is what crashed a walking phone). The
//! fast lane (`verse_rt`) carries movement over direct QUIC datagrams; THIS lane
//! is its gossip twin for when a direct datagram link isn't up yet: everyone in
//! a world joins ONE shared topic and a single broadcast reaches them all.
//!
//! AUTHENTICATED **and** (for private worlds) ENCRYPTED + ROSTER-GATED:
//!   * EVERY frame is Ed25519-signed by the sender's identity seed and verified
//!     on receive against the pubkey embedded in the sender's did:key, so a peer
//!     cannot forge movement/chat attributed to another DID (the audit's
//!     spoofing finding) and stale frames are rejected; AND
//!   * for a PRIVATE/invited world the frame payload is symmetrically sealed
//!     under a per-MEMBERSHIP key (NOT the public world name) and the gossip
//!     TOPIC is derived from that key, so the world is unreadable to anyone who
//!     merely learns/guesses the world name; AND
//!   * the receive path is ROSTER-GATED (mirroring `verse_rt::bind`): a frame is
//!     deposited only if its authenticated `sender_did` is a member of the world
//!     we joined with — so even a key-knowing outsider cannot inject presence and
//!     a non-member's frames are dropped.
//!
//! F-VERSE-NAMEKEY (round-2 / F-13): the previous design derived BOTH the key and
//! the topic from the public world NAME alone (`blake3(name)`), and the receive
//! path had NO roster gate — so anyone who learned or guessed a world name could
//! derive the key and read or inject presence. That was a real leak for PRIVATE
//! worlds. The fix:
//!   * PRIVATE worlds (the default — e.g. "home", a user's invite-only yard):
//!     the key is derived from a PER-MEMBERSHIP SECRET — the canonical sorted set
//!     of member did:keys (which is private knowledge held only by peers that
//!     completed the sealed DM-lane invite/accept handshake) plus the world name
//!     and an admin-rotatable EPOCH. A name-guesser who does not know the exact
//!     membership set cannot derive the key or find the topic. A roster gate on
//!     receive is the belt to that braces.
//!   * PUBLIC worlds (explicitly listed in `is_public_world` — e.g. "city" /
//!     Ela City, a shared open plaza where everyone is meant to see everyone):
//!     this is PUBLIC BY DESIGN, so we KEEP name-derivation and DROP THE
//!     ENCRYPTION PRETENSE — the seal there protects nothing (every roamer can
//!     derive the key), so we document it as a plaza, not a private room. Frames
//!     are still Ed25519-signed (no DID spoofing) and the public roster is open.
//!
//! Guarantees, by construction (see `Carrier::subscribe_ephemeral`):
//!   * no Double-Ratchet seal — raw `encode_wire` framing; payload is sealed with
//!     a cheap symmetric key, NOT the ratchet (no per-tick disk write);
//!   * no `broker.json` write on receive — the receiver hands frames straight to
//!     an in-RAM inbox, it never touches the broker;
//!   * not persisted — the topic is never recorded in `broker.subscriptions`, so
//!     it is NOT re-joined on the next boot (nothing survives a restart). Because
//!     NOTHING here is persisted to disk, no new `#[serde(default)]` fields are
//!     introduced (there is no on-disk shape to keep legacy-compatible);
//!   * received frames are deposited into the SAME inbox Godot already drains via
//!     the fast lane (`verse_rt::deposit`), so the game sees gossip and datagram
//!     movement through one unchanged path.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use base64::engine::general_purpose::STANDARD as B64S;
use base64::Engine as _;
use iroh_gossip::api::GossipSender;

use crate::carrier::Carrier;

/// Worlds that are PUBLIC BY DESIGN: a shared open plaza where every roamer is
/// meant to see and be seen (Ela City). For these we keep name-derivation and
/// make no confidentiality claim — the "seal" protects nothing when every
/// passer-by can derive the key, so we document it as a plaza rather than pretend
/// it is a private room. Authenticity (Ed25519 signatures) still holds, so a peer
/// in the plaza still cannot forge frames attributed to another DID. Everything
/// NOT in this list is treated as PRIVATE/invite-only (membership-secret key +
/// roster gate).
fn is_public_world(world: &str) -> bool {
    matches!(world, "city")
}

/// Per-sender highest timestamp accepted so far. Closes the replay window left by
/// the (deliberately generous) 120s freshness check: a captured frame replayed
/// within that window is dropped here because its `ts` no longer advances. This is
/// in-RAM only and movement is "newest wins" anyway, so dropping non-advancing
/// frames never costs a legit position update (the next real frame is newer).
static SEEN: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
fn seen() -> &'static Mutex<HashMap<String, i64>> {
    SEEN.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Hard cap on the SEEN dedup map. On a PUBLIC plaza every attacker-minted DID
/// would otherwise add an entry that never expires, growing the map unbounded.
/// Dedup only needs entries inside the freshness window (a frame older than the
/// window is already dropped by the freshness check), so pruning stale entries
/// is lossless; the cap is a backstop against a flood of fresh forged DIDs.
const MAX_SEEN: usize = 4096;

/// The live lane: the broadcast handle, the world topic it's bound to, the
/// per-world/per-membership symmetric key, the AUTHORIZED member roster (the
/// receive gate), and whether this world is public (no confidentiality claim).
struct Live {
    sender: Arc<GossipSender>,
    topic: String,
    key: [u8; 32],
    /// did:keys authorized to inject into this world. For a private world this is
    /// the canonical membership set fed by the sealed DM handshake; a frame whose
    /// authenticated sender is NOT in here is dropped on receive (mirrors
    /// `verse_rt::bind`'s roster gate). For a public world it is empty and the
    /// gate is open (see `roster_open`).
    roster: Arc<HashSet<String>>,
    /// True for public-by-design worlds: the roster gate is open and there is no
    /// confidentiality claim (key is name-derived and shared by every roamer).
    roster_open: bool,
}

static LIVE: OnceLock<Mutex<Option<Live>>> = OnceLock::new();
/// Bumped on reset()/world-switch so the old receiver loop dies with the session.
static GEN: AtomicU64 = AtomicU64::new(0);

fn live() -> &'static Mutex<Option<Live>> {
    LIVE.get_or_init(|| Mutex::new(None))
}

/// Build the CANONICAL membership set for a private world: our own did:key plus
/// every present member's did:key, de-duplicated and SORTED so that every member
/// independently derives the IDENTICAL set (and thus the identical key/topic),
/// regardless of which subset each device happens to list. Empty/blank entries
/// are dropped. This set is private knowledge — it is only ever assembled from
/// peers that completed the sealed DM-lane invite/accept handshake — so it is the
/// per-membership secret that replaces the public world name as the key source.
fn canonical_members(me: &str, members: &[String]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    if !me.is_empty() {
        set.insert(me.to_string());
    }
    for m in members {
        if !m.is_empty() {
            set.insert(m.clone());
        }
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

/// Per-world symmetric key.
///
/// * PUBLIC world: derived from the world NAME (every roamer shares it — by
///   design, no confidentiality claim). Domain-separated under `.../public/v1`.
/// * PRIVATE world: derived from the PER-MEMBERSHIP SECRET — the canonical sorted
///   member did:key set — bound to the world name and an admin-rotatable `epoch`.
///   A passive observer who guesses the world name but does NOT know the exact
///   membership set can neither derive the key nor find the topic. Bumping
///   `epoch` (admin rotation) deterministically re-keys the whole membership.
///
/// Domain-separated so it can never collide with any other key derivation.
fn world_key(world: &str, members: &[String], epoch: u32) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    if is_public_world(world) {
        // PUBLIC plaza: name-derived, no secrecy claimed. Distinct domain so a
        // public-world key can never equal a private-world key for the same name.
        h.update(b"hey/verse-gossip/world-key/public/v1\x00");
        h.update(world.as_bytes());
    } else {
        // PRIVATE/invited world: membership-secret derived + epoch-rotatable.
        h.update(b"hey/verse-gossip/world-key/membership/v1\x00");
        h.update(world.as_bytes());
        h.update(b"\x00");
        h.update(&epoch.to_le_bytes());
        // Each member did:key, length-prefixed so distinct rosters can't collide
        // (e.g. ["ab","c"] must not hash like ["a","bc"]).
        for m in members {
            h.update(&(m.len() as u32).to_le_bytes());
            h.update(m.as_bytes());
        }
    }
    *h.finalize().as_bytes()
}

/// Per-world presence namespace, derived from the per-world/per-membership key
/// (NOT the literal world name) so the world name never appears on the wire and
/// the topic is not guessable without knowing the key (which, for a private
/// world, requires knowing the membership secret). Domain-separated from DM
/// queues (`q/<id>`) and feed topics (`hey-social/feed/...`) by its literal prefix.
fn verse_topic(key: &[u8; 32]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"hey/verse-gossip/topic/v1\x00");
    h.update(key);
    format!("hey/verse/room/{}", hey_core::identity::bytes_to_hex(h.finalize().as_bytes()))
}

/// Canonical bytes signed over each frame — domain-separated, binding the
/// sender DID + world topic + timestamp + (sealed) content so a frame cannot be
/// replayed into another world or re-attributed to another DID. We sign over the
/// on-wire ciphertext so the signature covers exactly what peers receive.
fn signing_bytes(sender_did: &str, topic: &str, ts: i64, content: &str) -> Vec<u8> {
    format!("hey/verse-gossip/1\u{0}{sender_did}\u{0}{topic}\u{0}{ts}\u{0}{content}").into_bytes()
}

/// Join (or switch to) the ephemeral movement topic for `world`, bootstrapping
/// with whatever contact tickets we hold for the currently-present peers.
///
/// * `me`        — our own did:key (stamped on outbound frames, used to drop our
///                 own echo, AND folded into the membership secret).
/// * `members`   — the did:keys of the present peers (the roster fed by the sealed
///                 DM-lane invite/accept handshake). Used BOTH as the per-
///                 membership key material (private worlds) and as the receive
///                 roster gate.
/// * `epoch`     — admin-rotatable key epoch for private worlds (bump to re-key
///                 the whole membership; pass 0 until an admin rotation UI exists).
/// * `bootstrap` — carrier tickets to seed the gossip mesh.
///
/// Idempotent while already on the same world+membership+epoch.
pub async fn join(
    carrier: Arc<Carrier>,
    world: String,
    me: String,
    members: Vec<String>,
    epoch: u32,
    bootstrap: Vec<String>,
) {
    let public = is_public_world(&world);
    // The canonical membership set is the per-membership secret (private worlds)
    // AND the receive roster (both worlds — empty/open for public). For a private
    // world we fold `me` + members into the key; for a public world the key is
    // name-only and the roster gate is open.
    let canon = canonical_members(&me, &members);
    let key = world_key(&world, &canon, epoch);
    let topic = verse_topic(&key);
    // The roster gate authorizes the OTHER members (we never gate ourselves; our
    // own echo is dropped by the `me` check). For a public world the gate is open.
    let roster: HashSet<String> = if public {
        HashSet::new()
    } else {
        canon.iter().filter(|d| *d != &me).cloned().collect()
    };
    // Already on this exact topic? Gossip is a self-healing plumtree mesh, so a
    // re-seed isn't needed — keep the live lane. (A membership/epoch change yields
    // a different key→topic, so it correctly falls through to a fresh subscribe.)
    if let Some(cur) = crate::lock_safe(live()).as_ref() {
        if cur.topic == topic {
            return;
        }
    }
    // First join, or a world/membership/epoch switch: bump GEN to kill any old
    // receiver loop, then subscribe fresh on the new topic.
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let me_echo = me;
    let topic_v = topic.clone();
    let key_v = key;
    let roster_arc = Arc::new(roster);
    let roster_recv = roster_arc.clone();
    let public_recv = public;
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
                // ROSTER GATE (mirrors verse_rt::bind): for a PRIVATE world, a
                // frame is accepted ONLY from a did:key in the membership roster
                // fed by the sealed DM handshake. This is the belt to the key's
                // braces: even a peer that somehow derives the key cannot inject
                // presence, and a non-member's frames are dropped before deposit.
                // For a PUBLIC plaza the gate is open by design.
                if !public_recv && !roster_recv.contains(&sender_did) {
                    return; // not an authorized member of this private world → drop
                }
                // Freshness: movement is perishable — reject stale/replayed frames.
                // Generous window so normal device clock skew never drops live frames;
                // movement replay is harmless anyway (the next real frame overwrites).
                let now = hey_core::plat::now_ms();
                if (now - ts).abs() > 120_000 {
                    return;
                }
                // Authenticity: verify the Ed25519 signature against the pubkey
                // EMBEDDED in the sender's did:key — an attacker cannot forge a
                // frame as another DID without its key. The signature covers the
                // on-wire (sealed) content.
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
                // Replay dedup: only AFTER the signature proves the DID is genuine
                // do we trust `ts`. Accept strictly-advancing timestamps only, so a
                // captured-and-replayed frame inside the 120s freshness window is
                // dropped. Recorded per authenticated sender so a forged DID can't
                // poison a real sender's last-seen.
                {
                    let mut s = crate::lock_safe(seen());
                    // Prune entries older than the freshness window: a frame whose
                    // last-seen `ts` is staler than that can never block a legit
                    // advancing frame (the freshness check already dropped it), so
                    // evicting it is lossless. Bounds the map on a public plaza
                    // where each attacker-minted DID would otherwise leak an entry.
                    let cutoff = now - 120_000;
                    s.retain(|_, last| *last >= cutoff);
                    // Backstop against a flood of fresh forged DIDs within the
                    // window: if still over the cap, evict oldest (smallest ts)
                    // until under it. FIFO/LRU on perishable movement state.
                    while s.len() >= MAX_SEEN {
                        if let Some(oldest) =
                            s.iter().min_by_key(|(_, &v)| v).map(|(k, _)| k.clone())
                        {
                            s.remove(&oldest);
                        } else {
                            break;
                        }
                    }
                    let last = s.entry(sender_did.clone()).or_insert(i64::MIN);
                    if ts <= *last {
                        return; // replayed / out-of-order stale frame → drop
                    }
                    *last = ts;
                }
                // Confidentiality: open the per-world/per-membership seal. A peer
                // that isn't a member can't derive `key_v`, so this fails → drop.
                // Empty ciphertext or any tamper also fails closed. (For a public
                // world this seal is name-derived and claims no secrecy — the
                // roster gate above is simply open there.)
                let plain = match B64S
                    .decode(content.as_bytes())
                    .ok()
                    .and_then(|blob| hey_core::crypto::open_at_rest(&key_v, &blob))
                    .and_then(|pt| String::from_utf8(pt).ok())
                {
                    Some(p) => p,
                    None => return,
                };
                crate::verse_rt::deposit(sender_did, plain);
            },
        )
        .await;
    if let Some(sender) = sender {
        *crate::lock_safe(live()) = Some(Live {
            sender: Arc::new(sender),
            topic,
            key,
            roster: roster_arc,
            roster_open: public,
        });
    }
}

/// Broadcast one movement payload (raw JSON) to the live world topic. Silent
/// no-op until a topic is joined. No ratchet seal, no disk — the payload is
/// sealed under the cheap per-world/per-membership symmetric key, then signed.
pub async fn send_all(payload: String) {
    let (sender, topic, key) = match crate::lock_safe(live()).as_ref() {
        Some(cur) => (cur.sender.clone(), cur.topic.clone(), cur.key),
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
    // Seal the payload under the per-world/per-membership key (ChaCha20-Poly1305,
    // fresh nonce) so only world members can read presence; the signature then
    // covers the exact ciphertext that goes on the wire.
    let sealed = B64S.encode(hey_core::crypto::seal_at_rest(&key, payload.as_bytes()));
    // N1 capability completeness: this is the one runtime path that signs with the
    // identity key outside the provider identity/sign op, so route it through the
    // same named gate. "identity"/"sign" is in CAPABILITIES, so this NEVER denies a
    // real presence frame (verse gossip is unchanged); it fails closed only if the
    // grant is ever revoked, and a denial is auto-audited by check(). No per-frame
    // success audit — presence frames are high-frequency and would flood the log.
    if crate::guard::check("identity", "sign").is_err() {
        return;
    }
    let sig = hey_core::identity::sign(&signing_bytes(&me, &topic, ts, &sealed), &id.seed());
    let bytes = crate::carrier::encode_wire(&sealed, &me, ts, &sig);
    let _ = sender.broadcast(bytes).await;
}

/// Tear down the lane (session emptied / world closed). Bumping GEN kills the
/// receiver loop; dropping LIVE releases the gossip subscription.
pub fn reset() {
    GEN.fetch_add(1, Ordering::SeqCst);
    *crate::lock_safe(live()) = None;
    crate::lock_safe(seen()).clear();
}

/// True if the gossip presence lane is currently joined to a world topic.
pub fn connected() -> bool {
    crate::lock_safe(live()).is_some()
}

/// Diagnostics: the number of did:keys currently authorized on the receive roster
/// (0 for a public plaza or while no private world is joined). Keeps `roster` /
/// `roster_open` live and gives the future admin UI a read into membership size.
pub fn roster_size() -> usize {
    match crate::lock_safe(live()).as_ref() {
        Some(cur) if !cur.roster_open => cur.roster.len(),
        _ => 0,
    }
}
