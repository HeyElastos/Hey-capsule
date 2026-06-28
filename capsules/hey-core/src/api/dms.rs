// Direct-message API with E2E hybrid post-quantum encryption.
//
// v2 (default for new contacts) — METADATA-SAFE per-pair queues.
//
//   Each contact has a private random 256-bit queue ID. The wire-level
//   topic is `hey-v0/q/<queue_id>` — the recipient's DID never appears
//   in the topic name, so the `peer` provider sees only opaque queue
//   traffic between random pseudonyms. Equivalent to SimpleX Chat's
//   unidirectional queue model adapted to Carrier gossipsub.
//
//   Sealed-sender envelope: every byte of {sender_did, signature, text}
//   lives INSIDE the ChaCha20-Poly1305 ciphertext. The provider sees
//   only `{ "type": "dm.v2", "envelope": HpqEnvelope }` — no DID, no
//   signature, no plaintext, no length-distinguishable metadata.
//
// v1 (legacy) — kept so existing contacts created before v2 still work.
//
//   Topic `hey-v0/dm/<recipient_did>` with the recipient's DID in the
//   path — leaks the social graph at the routing layer. We keep
//   receiving on this topic for back-compat, but new contacts always
//   use v2.
//
// Bootstrap problem solved: the FIRST message between strangers is
// negotiated via an OOB invite link, not a plaintext fallback. The link
// carries Alice's pubkeys + queue_id; Bob's reply carries his. No
// plaintext is ever sent over the wire.
//
// Storage:
//   Hey/dm/contacts.json      — [ Contact { did, queue stuff, ... } ]
//   Hey/dm/by-did/<did>.json  — [ { id, text, ts, mine, encrypted } ]
//   Hey/dm/expiry.json        — per-contact TTL
//   Hey/dm/peer-keys.json     — DEPRECATED (kept readable for migration)

use base64::engine::general_purpose::STANDARD as B64;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine as _;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::api::profile::ensure_profile;
use crate::crypto::{self, HpqEnvelope, UserKeys};
use crate::events::canonicalize;
use crate::identity::{bytes_to_hex, did_key_to_public_key, hex_to_bytes, sign, verify};
use crate::runtime::{peer, storage, RuntimeError};
use crate::session;

const CONTACTS_FILE: &str = "dm/contacts.json";
const PEER_KEYS_FILE: &str = "dm/peer-keys.json";
const EXPIRY_FILE: &str = "dm/expiry.json";
/// F-BLOCK-CALL-RING: persisted engine-level block set. Block was previously only
/// a Kotlin UI pref, so a blocked DID's sealed DMs (incl. the `hey-call` ring
/// control message) still stored + surfaced — the device rang. This is the
/// source of truth the receive path FAILS CLOSED on. A flat JSON array of DIDs,
/// mirroring `dm/contacts.json` persistence.
const BLOCKED_FILE: &str = "dm/blocked.json";

/// F-FOLLOW-PoP: STABLE sentinel `send_message` returns when a contact's keys
/// arrived from an unverified, unsigned source and the first send needs explicit
/// user confirmation. The native UI layer (Kotlin/Swift) matches this exact
/// string to raise the verify-or-send-anyway prompt instead of showing a generic
/// error. Do NOT change the text without updating the consumers.
pub const NEEDS_VERIFY_BEFORE_SEND: &str = "needs_verify_before_send";

/// Verse lane: ephemeral world-presence traffic (invites, movement, in-world
/// chat). Sealed + ratcheted EXACTLY like a DM on the wire, but on receive it
/// is diverted into an in-memory inbox — never stored in the conversation,
/// never counted as unread, never notified. Game traffic, not messages.
pub const VERSE_PREFIX: &str = "\u{1}hey-verse:1:";

fn verse_inbox() -> &'static std::sync::Mutex<std::collections::VecDeque<(String, String)>> {
    static INBOX: std::sync::OnceLock<
        std::sync::Mutex<std::collections::VecDeque<(String, String)>>,
    > = std::sync::OnceLock::new();
    INBOX.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()))
}

fn verse_push(from: &str, payload_b64: &str) {
    if let Ok(mut q) = verse_inbox().lock() {
        q.push_back((from.to_string(), payload_b64.to_string()));
        while q.len() > 512 {
            q.pop_front();
        }
    }
}

/// Drain everything queued on the verse lane: (sender did, base64 payload).
pub fn verse_drain() -> Vec<(String, String)> {
    verse_inbox()
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

/// v2 queues used to be `hey-v0/q/<rnd>`. We dropped the `hey-v0/`
/// prefix so an observer of the peer provider can't pick Hey-app
/// traffic out of arbitrary queue traffic by topic-name shape. Random
/// 256-bit ids still need a routing prefix; one ASCII char is enough.
const TOPIC_PREFIX_V2: &str = "q";

const KIND_MESSAGE: &str = "message";
const KIND_HANDSHAKE: &str = "handshake";
/// Sent by Alice on Bob's queue right after she processes his
/// handshake. Carries a fresh Alice-side queue id; lets Alice retire
/// the original invite queue so a leaked link can't be reused.
const KIND_WELCOME: &str = "welcome";

/// Invite-link wire version. Bumping this invalidates old links so we
/// can safely change the embedded JSON shape.
const INVITE_LINK_VERSION: u8 = 2;
/// How long an invite link is valid for, in ms. Pasting after this
/// expires fails with a clear error. 24 hours felt like the right
/// trade-off between "share now, accept later" and the leak window.
const INVITE_TTL_MS: i64 = 24 * 60 * 60 * 1000;

// ── Double Ratchet (M6) ──────────────────────────────────────────────
//
/// Per-contact ratchet state lives in its OWN file under this dir, NOT on
/// DmContact — so the (potentially large) skipped-keys blob never rides the
/// whole-contacts.json rewrite that runs on EVERY message (must-fix #7).
const RATCHET_DIR: &str = "dm/ratchet";
/// Max messages we will skip (and derive keys for) in a SINGLE chain advance.
/// A cleartext header claiming a jump larger than this is rejected BEFORE any
/// KDF runs, so a forged counter can't make us burn unbounded CPU (must-fix #7).
const MAX_SKIP: u32 = 1000;
/// Hard cap on stored out-of-order keys (FIFO eviction). Bounds memory.
const MAX_SKIPPED_KEYS: usize = 2000;
/// Skipped keys older than this are evicted — a message that never arrived.
const SKIPPED_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

// ── Continuous queue rotation (caveat 1) ─────────────────────────────
// Each side rotates its OWN inbound queue periodically so the relay can't build
// a long traffic history against one durable per-conversation handle. Reuses the
// existing `welcome` control message to tell the peer the new queue. HONEST
// LIMIT: rotation reduces LINKABILITY of a stable handle, but the relay still
// sees "a packet moved to a topic" and can TIMING-CORRELATE a retire→relight —
// only the garlic overlay removes that. Partial close, pairs with the overlay.
/// Rotate the inbound queue after this many received messages on it...
const QUEUE_ROTATE_MSGS: u32 = 200;
/// ...or after this long, whichever comes first.
const QUEUE_ROTATE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Never rotate more than once per this interval (anti-thrash floor).
const QUEUE_ROTATE_FLOOR_MS: i64 = 60 * 60 * 1000;
/// Keep polling a retired queue this long after rotating, so in-flight messages
/// the peer sent before processing our `welcome` aren't lost (≫ the 5 s poll).
const QUEUE_GRACE_MS: i64 = 60 * 60 * 1000;
/// F-LEGACY-PAIR-TOPIC: after a peer advertises salted support (`peer_salted`),
/// keep SUBSCRIBING the leaky legacy deterministic pair topic for this long, then
/// drop it. The window only needs to cover the round-trip for the peer to see OUR
/// `sc:true` and migrate ITS sends to the salted topic; a generous 24 h means an
/// in-flight legacy send is never stranded while still closing the long-lived
/// DID-derivable metadata leak. (Sends already migrate immediately on peer_salted.)
const LEGACY_TOPIC_GRACE_MS: i64 = 24 * 60 * 60 * 1000;

fn conv_path(did: &str) -> String {
    let safe = did.replace(['/', ':'], "_");
    format!("dm/by-did/{safe}.json")
}

fn now_ms() -> i64 {
    crate::plat::now_ms()
}

// ── Receive-path safety bounds (HARDENING) ───────────────────────────
//
// Local retention + anti-DoS caps applied ONLY on the receive WRITE path.
// They never signal peers, never drop unsynced OUTBOUND messages, and never
// shrink already-stored history below the cap on read. All are generous so the
// legit UX (long chats, large transfers, big-but-bounded groups) is unaffected.

/// Max messages retained per 1-to-1 / group conversation log. On the receive
/// write path we keep the NEWEST `MAX_CONV_MSGS` (oldest pruned) so an attacker
/// who floods a queue can't grow a conversation file without bound. 5000 is far
/// above any human chat session; pruning only ever touches stored INCOMING
/// history (the trimmed tail is old, already-read messages).
const MAX_CONV_MSGS: usize = 5000;

/// A received message timestamp is CLAMPED into a sane window: at most
/// `TS_FUTURE_SKEW_MS` ahead of local now (a far-future ts would otherwise pin a
/// conversation to the top of every list forever and defeat TTL pruning — it's
/// security-load-bearing). A negative/zero ts falls back to now.
const TS_FUTURE_SKEW_MS: i64 = 24 * 60 * 60 * 1000; // 24h forward tolerance

/// Reject (do NOT truncate) an inbound group roster larger than this — a forged
/// huge roster would otherwise force thousands of pairwise key bootstraps. Well
/// above any real group.
const MAX_GROUP_MEMBERS: usize = 1024;
/// Cap on the number of groups we will materialise locally. A new (previously
/// unknown) group beyond this count is dropped; existing groups always update.
const MAX_GROUPS: usize = 4096;
/// Cap on the number of attachments carried by a single received message — a
/// forged message can't pin us into thousands of fetches.
const MAX_ATTACHMENTS_PER_MSG: usize = 64;
/// Cap on stored reactions per group conversation. One reaction per
/// (message_id, sender_did) is retained, so this bounds the per-group reactions
/// file: a member can't grow it without bound by reacting to fabricated message
/// ids. Well above any real conversation's reaction volume. A NEW reaction past
/// the cap is dropped; replacing/removing an existing reaction always proceeds.
const MAX_GROUP_REACTIONS: usize = 8192;

/// Cap on stored reactions per 1:1 DM conversation (mirrors MAX_GROUP_REACTIONS).
/// Bounds the per-peer reactions file so a remote peer can't grow it without bound
/// by reacting to fabricated message ids. A NEW reaction past the cap is dropped;
/// replacing/removing an existing reaction always proceeds.
const MAX_DM_REACTIONS: usize = 512;

/// Clamp a received message timestamp into a sane window (see TS_FUTURE_SKEW_MS).
fn clamp_recv_ts(ts: i64) -> i64 {
    let now = now_ms();
    if ts <= 0 {
        return now;
    }
    let ceiling = now.saturating_add(TS_FUTURE_SKEW_MS);
    if ts > ceiling {
        ceiling
    } else {
        ts
    }
}

/// Trim a conversation log to the newest `MAX_CONV_MSGS` entries IN PLACE. Keeps
/// newest (the tail), drops the oldest head. No-op when already within bounds, so
/// legacy logs are never rewritten until they actually exceed the cap.
fn cap_conv_log(conv: &mut Vec<DmMessage>) {
    if conv.len() > MAX_CONV_MSGS {
        let drop = conv.len() - MAX_CONV_MSGS;
        conv.drain(0..drop);
    }
}

/// In-memory O(1) dedup index per conversation: a `HashSet` for the membership
/// test plus a `VecDeque` for bounded FIFO eviction. A redelivered envelope (the
/// outbox retries + gossip re-delivers, so duplicates are routine) is rejected in
/// O(1) here BEFORE the O(n) log scan / disk read. Bounded per conversation so it
/// can't grow without bound. Best-effort: a process restart loses it, after which
/// the durable per-log scan still catches the duplicate — so this never causes a
/// MISSED dedup, only skips redundant work. The conv key is the partner DID
/// (1-to-1) or "g:"+gid (group).
#[derive(Default)]
struct DedupRing {
    set: std::collections::HashSet<String>,
    order: std::collections::VecDeque<String>,
}
fn dedup_index() -> &'static std::sync::Mutex<HashMap<String, DedupRing>> {
    static IDX: std::sync::OnceLock<std::sync::Mutex<HashMap<String, DedupRing>>> =
        std::sync::OnceLock::new();
    IDX.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// True if `id` was already seen for `conv_key` (O(1) HashSet lookup). When NOT
/// seen, records it (bounded ring, oldest evicted) and returns false.
fn dedup_seen(conv_key: &str, id: &str) -> bool {
    let Ok(mut map) = dedup_index().lock() else {
        return false; // lock poisoned — defer to the durable scan
    };
    let ring = map.entry(conv_key.to_string()).or_default();
    if ring.set.contains(id) {
        return true;
    }
    ring.set.insert(id.to_string());
    ring.order.push_back(id.to_string());
    while ring.order.len() > MAX_CONV_MSGS {
        if let Some(old) = ring.order.pop_front() {
            ring.set.remove(&old);
        }
    }
    false
}

/// Process-global async gate serializing the read-modify-write of
/// `dm/contacts.json` — the SAME pattern as `outbox::outbox_gate`. On NATIVE the
/// engine runs across multiple OS threads (the peer_receiver poll thread drives
/// handshake/welcome/queue-rotation/continuity-pin writes while JNI threads
/// mutate contacts), so two unsynchronized read→modify→write cycles can
/// interleave and clobber each other (the continuity-pin / receive_handshake
/// lost-update race). The gate makes those receive-path RMW cycles atomic with
/// respect to one another. It is an async (no-OS-thread) mutex, so it is an
/// uncontended no-op on single-threaded wasm. NOT re-entrant: a function holding
/// it must NOT call another gated function.
fn contacts_gate() -> &'static futures_util::lock::Mutex<()> {
    static G: std::sync::OnceLock<futures_util::lock::Mutex<()>> = std::sync::OnceLock::new();
    G.get_or_init(|| futures_util::lock::Mutex::new(()))
}

/// SAFETY NUMBER (F-12) over the PINNED ENCRYPTION MATERIAL of both parties —
/// Signal-style. Hashes the SORTED pair of `{did, x25519_pub, ml_kem_pub}` tuples
/// (mine + the contact's pinned keys), so both sides compute the IDENTICAL number
/// regardless of who calls. Bound to the actual key material (not the DID alone),
/// so a key-substitution MITM changes the number and the user's OOB comparison
/// catches it — and `key_changed` stays meaningful. Returns "" when either side's
/// pinned keys are not (yet) known (legacy/keyless contact → nothing to compare).
pub async fn safety_number(did: &str) -> String {
    use sha2::{Digest, Sha256};
    let Some(my_real) = my_pubkeys().await else {
        return String::new();
    };
    let Some(c) = find_contact(did).await else {
        return String::new();
    };
    let Some(theirs) = c.peer_pubkeys.clone() else {
        return String::new();
    };
    // Incognito/Anonymous removed — always hash our real session identity against the peer's.
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    let mine = my_real;
    // Each side = a stable triple; sort the two triples so the hash is symmetric.
    let me_t = format!("{my_did}|{}|{}", mine.x25519_pub_b64, mine.ml_kem_pub_b64);
    let them_t = format!("{did}|{}|{}", theirs.x25519_pub_b64, theirs.ml_kem_pub_b64);
    let (a, b) = if me_t <= them_t { (me_t, them_t) } else { (them_t, me_t) };
    let mut h = Sha256::new();
    h.update(b"hey-safety-number:v1\0");
    h.update(a.as_bytes());
    h.update(b"\0");
    h.update(b.as_bytes());
    let digest = h.finalize();
    // 60-digit decimal, grouped 5x12 — the familiar Signal layout.
    let mut groups: Vec<String> = Vec::with_capacity(12);
    for chunk in digest[..30].chunks(5) {
        let mut acc: u64 = 0;
        for &byte in chunk {
            acc = acc.wrapping_mul(256).wrapping_add(byte as u64);
        }
        groups.push(format!("{:05}", acc % 100000));
    }
    groups.join(" ")
}

fn random_hex(n_bytes: usize) -> String {
    let mut buf = vec![0u8; n_bytes];
    OsRng.fill_bytes(&mut buf);
    bytes_to_hex(&buf)
}

// ── Contact ──────────────────────────────────────────────────────────
//
// Persisted in dm/contacts.json. A v2 contact has Some(queue stuff);
// a v1 (legacy) contact has None and the old hey-v0/dm/<did> path is
// used. Migration is incremental: we never auto-upgrade a v1 contact
// in place — the upgrade happens when the user generates a fresh invite.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactStatus {
    /// We minted an invite for this contact and are waiting for their
    /// handshake. Outgoing messages are queued (we don't know their
    /// queue/keys yet); UI shows "Awaiting reply…".
    PendingInvite,
    /// They sent a handshake; we have their queue + pubkeys; messages
    /// can flow in both directions.
    Active,
}

impl Default for ContactStatus {
    fn default() -> Self {
        ContactStatus::Active
    }
}

/// Which identity OUR side of a conversation presents to the peer.
///
/// SimpleX-style "incognito": Regular uses the stable, federated did:key
/// from the session (cross-app, verifiable — the default); Anonymous uses
/// a per-contact ephemeral identity that is never linked to the real DID.
/// The mode only changes WHICH key signs the inner payload and WHICH
/// pubkeys/DID/name we advertise — the sealed-sender envelope (crypto.rs)
/// already carries nothing about the sender, so this is sufficient for
/// identity anonymity. It does NOT hide network metadata (node id / IP
/// still traverse Carrier — that needs the garlic overlay).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityMode {
    /// Stable did:key from the session — the ONLY mode. Incognito/Anonymous mode was
    /// removed (it was a recurring source of delivery + safety-number bugs while regular
    /// chats work reliably). `#[serde(other)]` makes this the catch-all so any contact
    /// previously persisted as `anonymous` loads cleanly as Regular instead of failing the
    /// whole contact-list deserialize.
    #[default]
    #[serde(other)]
    Regular,
}

/// A per-contact ephemeral identity used in Anonymous mode. Minted fresh
/// for ONE contact (never reused — that is what makes our anonymous
/// contacts mutually unlinkable) and never derived from the session
/// identity. Persisted locally on the contact; only its PUBLIC projection
/// (did + pubkeys) is ever put on the wire, in the invite/handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnonIdentity {
    /// 32-byte Ed25519 seed (hex) — yields both the ephemeral signing key
    /// and (via x25519_from_seed) the ephemeral X25519 key.
    pub seed_hex: String,
    /// Ephemeral ML-KEM-768 secret (base64) — decrypts traffic the peer
    /// sealed to our advertised ephemeral pubkey.
    pub ml_kem_secret_b64: String,
    /// Ephemeral ML-KEM-768 public (base64) — advertised in the invite /
    /// handshake so the peer encrypts to this key, not our real one.
    pub ml_kem_public_b64: String,
    /// The ephemeral did:key (derived from seed_hex), cached so we present
    /// a stable pseudonym to this one contact without re-deriving each send.
    pub did: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmContact {
    pub did: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "lastTs")]
    pub last_ts: i64,
    #[serde(default, rename = "lastPreview")]
    pub last_preview: String,
    #[serde(default)]
    pub unread: u32,

    // ── v2 fields. None ⇒ legacy v1 contact (route via hey-v0/dm/<did>).
    /// 256-bit random hex — topic we listen on for messages from this
    /// contact. We share this in our outbound invite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_inbound_queue: Option<String>,
    /// 128-bit random hex — opaque consumer_id we present to the peer
    /// provider when reading from `my_inbound_queue`. Unlinkable to DID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_recv_pseudonym: Option<String>,
    /// 256-bit random hex — their queue (we publish here when sending
    /// to them). Filled in when their handshake arrives, or when WE
    /// accept their invite link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub their_inbound_queue: Option<String>,
    /// 128-bit random hex — opaque sender_id we present to the peer
    /// provider when publishing to `their_inbound_queue`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_send_pseudonym: Option<String>,
    /// The peer's gossip node ticket (base32 EndpointAddr from their invite or
    /// handshake). We dial it whenever we (re)join a queue we SEND on, so the
    /// cross-runtime mesh re-forms after the peer rotates to a fresh queue.
    /// None ⇒ same-runtime or legacy contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_ticket: Option<String>,
    /// F-OWNER-TICKET-PoP: true ⇒ `peer_ticket` was asserted by THIS contact
    /// itself (verified `sender_did == did` on a message/handshake), NOT by a
    /// group owner's roster bootstrap or contact-ticket poison. The group-call
    /// dial anchor uses ONLY a self-asserted ticket, so a malicious owner can't
    /// redirect a member's media stream to a non-member (Eve). Defaults false so
    /// old stored contacts + owner-bootstrapped tickets fail closed for the dial
    /// until the member self-asserts (auto on their next message).
    #[serde(default)]
    pub ticket_self_asserted: bool,
    /// Their X25519 + ML-KEM pubkeys, cached at handshake time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_pubkeys: Option<PeerKeys>,
    /// F-ROSTER-KEYPOISON: this contact's SELF-asserted proof-of-possession over
    /// `canonical_member_pop(did, peer_pubkeys)` — captured when the contact
    /// self-asserts its keys (signed follow/invite). Carried forward into any
    /// group roster we build that includes them (`roster_member`), so OTHER
    /// members can verify the keys are genuinely this member's (not owner-forged)
    /// before pinning them as a sealing key. None ⇒ no PoP captured yet (legacy /
    /// keyless contact); the roster entry then pins discovery-only on the recipient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_pop: Option<String>,
    /// Lifecycle flag. Default for legacy load is Active so existing
    /// contacts keep working.
    #[serde(default)]
    pub status: ContactStatus,

    /// Identity OUR side presents to this contact. Defaults to Regular for
    /// every contact created before Anonymous mode (no field in old JSON).
    #[serde(default)]
    pub mode: IdentityMode,
    /// The ephemeral identity backing `mode == Anonymous`. None ⇒ Regular.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anon_identity: Option<AnonIdentity>,

    /// STICKY: true once a Double Ratchet was bootstrapped with this contact
    /// (both sides advertised ratchet support in the invite + handshake). Set
    /// ONCE at bootstrap and NEVER cleared — so a contact can't be silently
    /// downgraded back to the no-PCS single-shot path (must-fix #6). The
    /// ratchet STATE itself lives in dm/ratchet/<did>.json, not here.
    #[serde(default)]
    pub ratchet_capable: bool,

    /// Key trust: true = the peer's encryption keys are SELF-asserted (invite
    /// handshake, their signed follow.request/friend-link, or a direct key
    /// confirmation). false = vouched by a THIRD PARTY (group roster) — pinned
    /// but unverified. We never let a roster assertion OVERWRITE verified keys,
    /// and a later self-assertion upgrades unverified→verified. Defaults true so
    /// pre-existing (invite-established) contacts stay verified across upgrade.
    #[serde(default = "default_true")]
    pub key_verified: bool,

    /// SAFETY-NUMBER ALARM: set true when this contact's pinned encryption keys
    /// CHANGED *after* the user had verified them (key_verified was true). The
    /// pin is NOT replaced — we keep refusing the new keys — but the UI raises a
    /// "safety number changed" warning so the user re-verifies out-of-band.
    /// Cleared by verify_contact() (the user re-verified). #[serde(default)] so
    /// existing stored contacts deserialize.
    #[serde(default)]
    pub key_changed: bool,

    /// STRONG verification: true ONLY after the user compared this contact's safety number
    /// OUT-OF-BAND (verify_contact). Unlike `key_verified` — which is also set provisionally by
    /// invite/bootstrap/signed-link paths — this is set by NOTHING but an explicit human verify, so
    /// it is the reliable signal of "I confirmed these exact keys belong to this person." The
    /// dup-merge handshake uses it to raise the `key_changed` alarm ONLY when keys change on a
    /// contact that was genuinely OOB-verified, avoiding the false alarms that keying off
    /// `key_verified` produced on normal re-pairs. #[serde(default)] ⇒ existing contacts deserialize.
    #[serde(default)]
    pub oob_verified: bool,

    // ── Continuous queue rotation. All default ⇒ a contact never rotated yet.
    /// When we last rotated `my_inbound_queue` (ms). 0 ⇒ clock not started; the
    /// first received message sets it to "now" so rotation can't fire instantly
    /// for a freshly-loaded legacy contact.
    #[serde(default)]
    pub my_queue_rotated_at: i64,
    /// Messages received on the CURRENT inbound queue since the last rotation.
    #[serde(default)]
    pub my_queue_msg_count: u32,
    /// Recently-retired inbound queues still inside the grace window — we keep
    /// polling them so in-flight messages aren't lost while the peer switches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retired_queues: Vec<RetiredQueue>,

    // ── F-11: salted per-pair topic. The legacy deterministic pair queue is
    // SHA256(DID‖DID) — computable by any DID-knower (a metadata leak). The
    // salted topic is HKDF'd over the per-pair X25519 static-static shared
    // secret, so only the two peers (who hold the private keys) can derive it.
    /// Cached salted per-pair topic (hex), HKDF'd over the X25519 static-static
    /// shared secret with this contact. Computed once (needs a provider DH) and
    /// pinned so the sync ownership check + listen/send paths reuse it. None ⇒
    /// not yet derivable (no peer keys / keyless-feed contact) ⇒ stay on the
    /// legacy deterministic topic. We ALWAYS keep listening on BOTH the legacy
    /// and the salted topic, so a message on either is delivered — never strand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salted_queue: Option<String>,
    /// True once we've received a message from this peer carrying the
    /// salted-support flag — i.e. the peer also listens on the salted topic.
    /// We only MIGRATE SENDS to the salted topic after this flips; until then we
    /// send on the legacy deterministic topic (guaranteed deliverable).
    #[serde(default)]
    pub peer_salted: bool,
    /// F-LEGACY-PAIR-TOPIC: when `peer_salted` first flipped true (ms). Historical
    /// bookkeeping only. NOTE (re-fix): this no longer drives the legacy-topic
    /// LISTEN abandonment grace — a peer-cooperation-gated clock let a peer that
    /// never sends `sc:true` keep us subscribed to the leaky legacy topic forever.
    /// The listen grace now runs off the SELF-owned `salted_self_ready_at` instead.
    /// 0 ⇒ not salted yet (legacy peer). serde-default so existing contacts load.
    #[serde(default)]
    pub peer_salted_at: i64,
    /// F-LEGACY-PAIR-TOPIC (re-fix): when WE first derived/pinned our own salted
    /// per-pair topic (ms). This is a SELF-owned, peer-INDEPENDENT event — it does
    /// NOT depend on the peer ever advertising `sc:true`. The legacy-topic LISTEN
    /// abandonment grace is driven from THIS stamp, so a non-cooperating peer can
    /// no longer keep us subscribed to the DID-derivable legacy pair topic forever.
    /// (SENDS still wait for `peer_salted` so we never publish onto a topic the
    /// peer doesn't join — only the leaky inbound SUBSCRIPTION times out on a
    /// self-event.) 0 ⇒ we haven't derived a salted topic yet (no peer keys /
    /// keyless contact) ⇒ keep the legacy subscription. serde-default so existing
    /// rosters load with 0 and start their clock on the next derivation.
    #[serde(default)]
    pub salted_self_ready_at: i64,

    /// F-FOLLOW-PoP gate. True ⇒ this contact's encryption keys were pinned from
    /// an UNVERIFIED, UNSIGNED source (an old unsigned key-bearing follow link
    /// carries no proof-of-possession over its PQ keys), and we have NOT yet
    /// sealed a message to them. The FIRST send is then blocked at the API
    /// (`send_message_inner` returns the sentinel error) until the user either
    /// confirms (`confirm_unverified_send`) or verifies the safety number
    /// (`verify_contact`), so an attacker can't get us to seal to substituted
    /// keys silently. Defaults FALSE so EVERY pre-existing contact — including
    /// ones with message history — is grandfathered and keeps sending. New links
    /// are signed (F-01) and pin verified, so they never set this.
    #[serde(default)]
    pub needs_verify_before_send: bool,
}

/// A rotated-away inbound queue, still polled until `retire_at + QUEUE_GRACE_MS`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetiredQueue {
    /// 256-bit hex — the old queue id (topic `q/<queue>`).
    pub queue: String,
    /// The consumer_id (pseudonym) we read it with.
    #[serde(default)]
    pub pseudonym: String,
    /// When we retired it (ms).
    pub retire_at: i64,
}

impl DmContact {
    /// True if this contact is fully wired up for v2 (we have their
    /// queue + pubkeys). False ⇒ either legacy v1 or pending invite.
    pub fn is_v2_active(&self) -> bool {
        self.peer_pubkeys.is_some()
            && self.their_inbound_queue.is_some()
            && self.my_inbound_queue.is_some()
    }

    /// True if `queue_id` is one we listen on for this contact — the current
    /// inbound queue OR a recently-retired one still inside the grace window.
    /// Lets messages that land on an old queue mid-rotation still route home.
    fn owns_inbound_queue(&self, queue_id: &str) -> bool {
        self.my_inbound_queue.as_deref() == Some(queue_id)
            || self.retired_queues.iter().any(|r| r.queue == queue_id)
    }

    /// Like `owns_inbound_queue` but ALSO matches the deterministic per-pair
    /// queue. Regular-mode contacts converge on `pair_inbound_queue(my_did,
    /// peer_did)` with no handshake dependency (the cross-runtime DM path), and
    /// the sender publishes there — so inbound ratchet/message traffic lands on
    /// that queue, not the minted `my_inbound_queue`. The pair queue mixes BOTH
    /// DIDs, so the owner check needs our own did, which the bare method lacks.
    /// (Without this, a delivered message is rejected "on an unowned queue" —
    /// the bug the deterministic-queue change left when it updated send +
    /// subscribe but not the receive-side ownership check.)
    fn owns_inbound_queue_with(&self, queue_id: &str, my_did: &str) -> bool {
        self.owns_inbound_queue(queue_id)
            || self.salted_queue.as_deref() == Some(queue_id)
            || (matches!(self.mode, IdentityMode::Regular)
                && pair_inbound_queue(my_did, &self.did) == queue_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmMessage {
    pub id: String,
    pub text: String,
    pub ts: i64,
    pub mine: bool,
    /// True if this message was delivered through the E2E envelope path,
    /// false if it was a plaintext bootstrap (only possible for legacy
    /// v1 contacts; v2 sends are always encrypted).
    #[serde(default)]
    pub encrypted: bool,
    /// E2E attachments (files/photos). Only the ciphertext lives in the blob
    /// store; the per-file key rides INSIDE this message's sealed payload, so
    /// the store/relay never sees plaintext. Fetched + decrypted on render.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Display name of the sender — only meaningful for GROUP messages (a 1-to-1
    /// conversation is implicitly between two known parties). Empty for DMs.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sender_name: String,
    /// CRYPTOGRAPHICALLY-VERIFIED sender did:key. For incoming messages this is the
    /// `inner.sender_did` that `verify_inner` tied to the signature (NOT a
    /// self-asserted display value); for our own (`mine`) messages it is our own
    /// did. Features that need the authentic author (group-call roster, tombstone/
    /// delete authority) MUST read this — never re-derive sender identity from an
    /// unverified payload field. Back-compat: legacy stored messages predate this
    /// field and deserialize to "" (serde default); consumers fall back to the old
    /// behaviour ONLY for such already-stored empty-`sender_did` history, never for
    /// newly-received messages (which always carry the verified value).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sender_did: String,
    /// Locally-pinned message — surfaced in the pinned bar. HEY chat upgrade.
    #[serde(default)]
    pub pinned: bool,
    /// Quoted-reply target (tap-a-bubble-to-reply). Carries the quoted message's id +
    /// author display + a short snippet, so the bubble renders the quote even when the
    /// original isn't in the recipient's local history. None = not a reply. Back-compat:
    /// older stored/received messages omit it and deserialize to None (serde default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<ReplyRef>,
}

/// A lightweight quote of the message a reply targets. Travels INSIDE the sealed
/// body (never on the wire in clear) so the recipient can render the quote without
/// needing the original message in local history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplyRef {
    /// Quoted message id (best-effort tap-to-jump; the quote renders regardless).
    pub id: String,
    /// Quoted author's display name ("" when it's the chat peer / yourself).
    pub author: String,
    /// Short preview of the quoted text (or a "📎 file" / "📷 photo" placeholder).
    pub snippet: String,
}

/// Parse a `reply` object out of an inbound sealed body into a [`ReplyRef`]. Bounds
/// the snippet/author so a forged reply can't bloat a stored message. None when absent.
fn parse_reply_ref(body: &Value) -> Option<ReplyRef> {
    let r = body.get("reply")?;
    Some(ReplyRef {
        id: r.get("id").and_then(Value::as_str).unwrap_or("").chars().take(80).collect(),
        author: r.get("author").and_then(Value::as_str).unwrap_or("").chars().take(80).collect(),
        snippet: r.get("snippet").and_then(Value::as_str).unwrap_or("").chars().take(160).collect(),
    })
}

/// Serialize a [`ReplyRef`] into the compact `reply` body object sent on the wire.
fn reply_ref_json(r: &ReplyRef) -> Value {
    json!({ "id": r.id, "author": r.author, "snippet": r.snippet })
}

/// Reference to one end-to-end-encrypted attachment. The bytes are NOT stored
/// here — `cid` points at the ciphertext in the content store and `key_b64`
/// (carried only inside the sealed message) decrypts it. Plaintext `name`/`mime`
/// /`size` are sealed too (never on the wire in clear).
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// Content-store ref to the CIPHERTEXT (IPFS CID today; an iroh-blobs ticket
    /// once that backend is registered — the upload/fetch boundary is abstracted).
    /// For a chunked attachment this is `chunks[0]` (back-compat); readers prefer
    /// `chunks` when present.
    pub cid: String,
    /// Ordered ciphertext-chunk CIDs. The encrypted blob is split into ≤1 MiB
    /// pieces so each content/publish stays under the runtime's 2 MB provider
    /// body limit — fetch concatenates them back before decrypt. Empty for a
    /// legacy single-blob attachment (use `cid`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<String>,
    /// Ordered iroh-blobs tickets — one per ciphertext chunk — for the DIRECT
    /// P2P attachment path (blobs-provider). Parallel to `chunks` but a
    /// different backend: when present, the recipient fetches each ticket
    /// straight from the holder over iroh-blobs (no IPFS add/pin/DHT) and
    /// concatenates before decrypt. Mutually exclusive with `cid`/`chunks` on a
    /// given attachment: `tickets` set => blobs path; empty => content-store
    /// (cid/chunks) or `inline_b64`. Empty on the wire when unused (back-compat).
    ///
    /// LIVENESS TRADEOFF: blobs is direct P2P, so the SENDER/holder must be
    /// ONLINE when the recipient fetches (no relay or pin cushion). The
    /// content-store path (`cid`/`chunks`) is offline-capable — pinned +
    /// federated — which is exactly why it is the FALLBACK when blobs is
    /// unavailable (see upload_attachment).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<String>,
    /// Base64 ChaCha20-Poly1305 key for this one file. Sealed E2E with the msg.
    pub key_b64: String,
    /// SMALL files ride INLINE here: base64 of the sealed ciphertext, carried
    /// inside the DM body so the file crosses over the CARRIER (fragmented like
    /// any oversized wire by `frag`) with NO IPFS round-trip. `Some` => decode
    /// + decrypt locally; `None` => fetch the ciphertext from the content store
    /// via `cid`/`chunks`. Omitted on the wire when absent (back-compat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_b64: Option<String>,
    /// STREAMED (torrent-style) attachment: true => `tickets[i]` are HPC1 PER-CHUNK
    /// segment frames (each its own AEAD), NOT pieces of one whole-file ciphertext.
    /// Sender encrypts + uploads, receiver fetches + decrypts, ONE chunk at a time
    /// (O(chunk) RAM) — this is what lifts the size cap. Default false => the
    /// existing whole-file path (inline / one-shot cid/chunks/tickets).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub streamed: bool,
    /// Number of HPC1 segments (== tickets.len()); sealed-trustworthy, so the
    /// receiver can hard-assert against truncation.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub chunk_count: u32,
    /// Base64 of the 12-byte per-file base nonce for the HPC1 segments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_nonce_b64: Option<String>,
}

impl Attachment {
    /// Hard upper bound on the CIPHERTEXT bytes we will accumulate during a
    /// whole-file fetch — a running anti-OOM ceiling checked after each chunk so a
    /// forged ref (small declared `size`, huge served chunks) is aborted BEFORE
    /// exhausting RAM. Generous: the whole-file plaintext ceiling
    /// (MAX_ATTACHMENT_BYTES) PLUS AEAD overhead slack (per-chunk tags across the
    /// chunk cap), so a legitimate at-ceiling transfer never trips it.
    fn cipher_fetch_ceiling(&self) -> u64 {
        // ChaCha20-Poly1305 adds 16 bytes/chunk; allow a generous 256 B/chunk of
        // framing slack across the chunk cap, plus 1 MiB fixed headroom.
        let n = self.chunks.len().max(self.tickets.len()).max(1) as u64;
        (MAX_ATTACHMENT_BYTES as u64)
            .saturating_add(n.saturating_mul(256))
            .saturating_add(1024 * 1024)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerKeys {
    pub x25519_pub_b64: String,
    pub ml_kem_pub_b64: String,
}

// ── Contact list CRUD ────────────────────────────────────────────────

pub async fn list_contacts() -> Vec<DmContact> {
    storage::read_json(CONTACTS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<Vec<DmContact>>(v).ok())
        .unwrap_or_default()
}

async fn write_contacts(list: &[DmContact]) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(list).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(CONTACTS_FILE, &v).await
}

// ── F-BLOCK-CALL-RING: engine-level block set ─────────────────────────
//
// Block is enforced HERE (the engine), not only in the Kotlin UI: a blocked
// DID's sealed DMs — including the `\u{1}hey-call:1:` ring control message that
// `social.rs call_poll` reads back to ring the device — must never be stored,
// notified, or create a conversation. Persisted as a flat JSON array of DID
// strings, the same `storage::read_json`/`write_json` shape as `contacts.json`.
// Consumers (lib.rs JNI hey_set_blocked/hey_is_blocked/hey_blocked_list) depend
// on these EXACT signatures.

/// True iff `did` is in the persisted engine block set. Fails closed only on the
/// block decision — a read error returns false (don't drop a legit message just
/// because the block file couldn't be read; the gate is additive moderation).
pub async fn is_blocked(did: &str) -> bool {
    if did.is_empty() {
        return false;
    }
    blocked_list().await.iter().any(|b| b == did)
}

/// Add (`blocked == true`) or remove (`blocked == false`) `did` from the engine
/// block set. Idempotent; empty `did` is a no-op. Best-effort persist (a write
/// error is swallowed — the UI mirror still reflects intent and the next call
/// retries).
pub async fn set_blocked(did: &str, blocked: bool) {
    if did.is_empty() {
        return;
    }
    let mut list = blocked_list().await;
    let present = list.iter().any(|b| b == did);
    if blocked && !present {
        list.push(did.to_string());
    } else if !blocked && present {
        list.retain(|b| b != did);
    } else {
        return; // no change — avoid a redundant write
    }
    if let Ok(v) = serde_json::to_value(&list) {
        let _ = storage::write_json(BLOCKED_FILE, &v).await;
    }
}

/// The engine block set as a `Vec<String>` of DIDs. Empty (fail closed to an
/// empty list) when nothing is blocked, the file is absent, or it can't be read.
pub async fn blocked_list() -> Vec<String> {
    storage::read_json(BLOCKED_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
}

pub async fn read_conversation(did: &str) -> Vec<DmMessage> {
    storage::read_json(&conv_path(did))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

async fn write_conversation(did: &str, msgs: &[DmMessage]) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(msgs).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(&conv_path(did), &v).await
}

// ── Per-message reactions ─────────────────────────────────────────────
//
// A reaction (👍/❤️/…) rides inside a NORMAL sealed message body as
// `{"reaction":{"message_id","emoji"}}` (plus the group roster for group
// reactions) — the SAME E2E ratchet/single-shot/fan-out path as a text message,
// so there is no new wire kind and a pre-reaction peer simply ignores it. One
// reaction per (sender, message); an empty emoji clears the sender's reaction
// (toggle off). Stored next to the conversation, keyed by the conversation
// partner's DID (1-to-1) or the group id.

/// One per-message reaction. `sender_did` is the reactor; `message_id` is the
/// target `DmMessage.id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReaction {
    pub message_id: String,
    pub emoji: String,
    pub sender_did: String,
    pub ts: i64,
}

fn reactions_path(did: &str) -> String {
    let safe = did.replace(['/', ':'], "_");
    format!("dm/by-did/{safe}.reactions.json")
}
fn group_reactions_path(gid: &str) -> String {
    let safe: String = gid.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    format!("dm/group-conv/{safe}.reactions.json")
}

/// All reactions in a 1-to-1 conversation (both mine and theirs).
pub async fn read_dm_reactions(did: &str) -> Vec<MessageReaction> {
    storage::read_json(&reactions_path(did))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}
async fn write_dm_reactions(did: &str, list: &[MessageReaction]) -> Result<(), String> {
    let v = serde_json::to_value(list).map_err(|e| format!("serialize: {e}"))?;
    storage::write_json(&reactions_path(did), &v)
        .await
        .map_err(|e| e.to_string())
}
/// All reactions in a group conversation.
pub async fn read_group_reactions(gid: &str) -> Vec<MessageReaction> {
    storage::read_json(&group_reactions_path(gid))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}
async fn write_group_reactions(gid: &str, list: &[MessageReaction]) -> Result<(), String> {
    let v = serde_json::to_value(list).map_err(|e| format!("serialize: {e}"))?;
    storage::write_json(&group_reactions_path(gid), &v)
        .await
        .map_err(|e| e.to_string())
}

/// Replace `sender_did`'s reaction on `message_id` (one per sender); an empty
/// emoji removes it. Idempotent — safe to re-apply on a redelivered wire.
///
/// `max` bounds the stored list AFTER removing the sender's prior reaction on
/// this message, so a replace/remove (or re-adding within budget) always
/// proceeds; only a brand-new reaction that would push the list past `max` is
/// dropped (anti-DoS for the per-group file). Pass `usize::MAX` to disable.
fn apply_reaction(
    list: &mut Vec<MessageReaction>,
    sender_did: &str,
    message_id: &str,
    emoji: &str,
    ts: i64,
    max: usize,
) {
    list.retain(|r| !(r.message_id == message_id && r.sender_did == sender_did));
    if !emoji.is_empty() {
        if list.len() >= max {
            return; // cap reached — drop this new reaction (replace/remove already applied)
        }
        list.push(MessageReaction {
            message_id: message_id.to_string(),
            emoji: emoji.to_string(),
            sender_did: sender_did.to_string(),
            ts,
        });
    }
}

/// Toggle MY reaction on a 1-to-1 message and tell the peer. Reacting with the
/// emoji I already have clears it. Returns my resulting emoji ("" = cleared).
pub async fn send_message_reaction(
    peer_did: &str,
    message_id: &str,
    emoji: &str,
) -> Result<String, String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    // CHAT-CAPABILITY ISOLATION: a reaction is user-authored 1:1 content (applied on the peer), so it
    // must be blocked for a follow-only contact exactly like a text message. This path does NOT go
    // through send_message_inner, so it needs its own gate. (Reactions are never SOH control DMs.)
    if !is_chat_enabled(peer_did).await {
        return Err("chat not enabled — scan their chat QR to start a private chat".into());
    }
    let emoji: String = emoji.chars().take(32).collect();
    let mut list = read_dm_reactions(peer_did).await;
    let mine = list
        .iter()
        .find(|r| r.message_id == message_id && r.sender_did == me.did_key)
        .map(|r| r.emoji.clone());
    let next = if mine.as_deref() == Some(emoji.as_str()) {
        String::new()
    } else {
        emoji
    };
    apply_reaction(&mut list, &me.did_key, message_id, &next, now_ms(), usize::MAX);
    write_dm_reactions(peer_did, &list).await?;
    let body = json!({ "reaction": { "message_id": message_id, "emoji": next } });
    send_body_to_contact(peer_did, &body).await?;
    Ok(next)
}

/// Toggle MY reaction on a group message and fan it out to the roster.
pub async fn send_group_message_reaction(
    group_id: &str,
    message_id: &str,
    emoji: &str,
) -> Result<String, String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let group = read_groups()
        .await
        .into_iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    let emoji: String = emoji.chars().take(32).collect();
    let mut list = read_group_reactions(group_id).await;
    let mine = list
        .iter()
        .find(|r| r.message_id == message_id && r.sender_did == me.did_key)
        .map(|r| r.emoji.clone());
    let next = if mine.as_deref() == Some(emoji.as_str()) {
        String::new()
    } else {
        emoji
    };
    apply_reaction(&mut list, &me.did_key, message_id, &next, now_ms(), MAX_GROUP_REACTIONS);
    write_group_reactions(group_id, &list).await?;
    let ctx = group_ctx(&group).await;
    let body = json!({ "reaction": { "message_id": message_id, "emoji": next }, "group": ctx });
    for m in &group.members {
        if m.did == me.did_key {
            continue;
        }
        let _ = send_body_to_contact(&m.did, &body).await;
    }
    Ok(next)
}

/// Apply a received reaction (carried in a normal message body's `reaction`
/// field). Routes to the group store when the body also carries a roster.
async fn handle_incoming_reaction(inner: &InnerPayload, react: &Value) -> Result<(), String> {
    let message_id = react.get("message_id").and_then(|v| v.as_str()).unwrap_or("");
    if message_id.is_empty() {
        return Err("reaction missing message_id".into());
    }
    let emoji: String = react
        .get("emoji")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(32)
        .collect();
    if let Some(group_ctx) = inner.body.get("group") {
        if let Some(gid) = upsert_group_from_ctx(group_ctx, &inner.sender_did).await {
            // SB-3 membership gate (mirrors store_incoming_group_message): accept a
            // group reaction only from a DID in the owner-controlled roster — or the
            // owner itself — and never from a kicked/blocked DID (F-07). An outsider
            // who learns the group id can no longer persist reactions (participation
            // spoof) or grow the per-group reactions file.
            let groups = read_groups().await;
            let Some(g) = groups.iter().find(|g| g.id == gid) else {
                return Ok(());
            };
            let is_member =
                g.created_by == inner.sender_did || g.members.iter().any(|m| m.did == inner.sender_did);
            if !is_member || is_group_barred(g, &inner.sender_did) {
                return Ok(());
            }
            // 1:1 engine block also bars a sender inside a SHARED group — a DID you
            // blocked must not reach you via group fan-out (parity with the DM gate).
            if is_blocked(&inner.sender_did).await {
                return Ok(());
            }
            let mut list = read_group_reactions(&gid).await;
            apply_reaction(&mut list, &inner.sender_did, message_id, &emoji, inner.ts, MAX_GROUP_REACTIONS);
            write_group_reactions(&gid, &list).await?;
        }
    } else {
        let mut list = read_dm_reactions(&inner.sender_did).await;
        // Cap an INCOMING (remote-driven) DM reaction so a peer can't grow the
        // per-peer reactions file without bound (mirrors the group cap). A new
        // reaction past the cap is dropped; replace/remove still proceeds.
        apply_reaction(&mut list, &inner.sender_did, message_id, &emoji, inner.ts, MAX_DM_REACTIONS);
        write_dm_reactions(&inner.sender_did, &list).await?;
    }
    Ok(())
}

pub async fn find_contact(did: &str) -> Option<DmContact> {
    list_contacts().await.into_iter().find(|c| c.did == did)
}

/// One-record-per-DID invariant: collapse any DUPLICATE contacts (legacy data from re-pair /
/// mutual-invite cycles created before receive_handshake learned to merge). For each DID it keeps
/// the MOST-COMPLETE record (keyed > v2-active > ratchet-capable > verified > newest) and unions the
/// losers' retired queues into it, so find_contact + ratchet routing always resolve the same record.
/// Idempotent; writes only when a duplicate existed. Call on boot. Returns true if it compacted.
pub async fn compact_contacts() -> bool {
    let _g = contacts_gate().lock().await;
    let list = list_contacts().await;
    let mut groups: std::collections::HashMap<String, Vec<DmContact>> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for c in list {
        if !groups.contains_key(&c.did) {
            order.push(c.did.clone());
        }
        groups.entry(c.did.clone()).or_default().push(c);
    }
    let mut had_dup = false;
    let mut out: Vec<DmContact> = Vec::with_capacity(order.len());
    for did in order {
        let mut group = groups.remove(&did).unwrap_or_default();
        if group.len() <= 1 {
            if let Some(c) = group.pop() {
                out.push(c);
            }
            continue;
        }
        had_dup = true;
        let score = |x: &DmContact| {
            (x.peer_pubkeys.is_some(), x.is_v2_active(), x.ratchet_capable, x.key_verified, x.last_ts)
        };
        let mut best_idx = 0;
        for i in 1..group.len() {
            if score(&group[i]) > score(&group[best_idx]) {
                best_idx = i;
            }
        }
        let mut best = group.remove(best_idx);
        for other in group {
            best.retired_queues.extend(other.retired_queues);
        }
        out.push(best);
    }
    if had_dup {
        out.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
        let _ = write_contacts(&out).await;
    }
    had_dup
}

/// Upsert one contact in the persisted list. Returns the resulting
/// (possibly-updated) record so callers can inspect queue/key state.
async fn upsert_contact_record(contact: DmContact) -> Result<DmContact, RuntimeError> {
    let _g = contacts_gate().lock().await; // serialize contact RMW (race fix)
    let mut list = list_contacts().await;
    let mut updated = contact;
    if let Some(pos) = list.iter().position(|c| c.did == updated.did) {
        // Preserve unread + ts from existing if the upsert doesn't
        // bring fresh ones (caller-controlled).
        let existing = &list[pos];
        if updated.last_ts == 0 {
            updated.last_ts = existing.last_ts;
        }
        if updated.last_preview.is_empty() {
            updated.last_preview = existing.last_preview.clone();
        }
        if updated.name.is_empty() {
            updated.name = existing.name.clone();
        }
        list[pos] = updated.clone();
    } else {
        list.push(updated.clone());
    }
    list.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    write_contacts(&list).await?;
    Ok(updated)
}

async fn touch_contact_message(
    did: &str,
    preview: &str,
    ts: i64,
    inc_unread: u32,
) -> Result<(), RuntimeError> {
    // A new message (in or out) re-opens a soft-deleted (hidden) chat — delete just wiped the
    // local history; the relationship lived on, so fresh activity brings the chat back.
    set_chat_hidden(did, false).await;
    let _g = contacts_gate().lock().await; // serialize contact RMW (race fix)
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        c.last_ts = ts;
        c.last_preview = preview.chars().take(140).collect();
        c.unread = c.unread.saturating_add(inc_unread);
    } else {
        // Legacy path: create a v1 contact on first sight.
        list.push(DmContact {
            did: did.into(),
            peer_ticket: None,
            ticket_self_asserted: false,
            name: String::new(),
            last_ts: ts,
            last_preview: preview.chars().take(140).collect(),
            unread: inc_unread,
            my_inbound_queue: None,
            my_recv_pseudonym: None,
            their_inbound_queue: None,
            my_send_pseudonym: None,
            peer_pubkeys: None,
            key_pop: None,
            status: ContactStatus::Active,
            mode: IdentityMode::Regular,
            anon_identity: None,
            ratchet_capable: false,
            key_verified: true,
            key_changed: false,
            oob_verified: false,
            my_queue_rotated_at: 0,
            my_queue_msg_count: 0,
            retired_queues: Vec::new(),
            salted_queue: None,
            peer_salted: false,
            peer_salted_at: 0,
            salted_self_ready_at: 0,
            needs_verify_before_send: false,
        });
    }
    list.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    write_contacts(&list).await
}

/// Like `touch_contact_message` but ALSO adopts the sender's live nickname in the
/// SAME read-modify-write — so a received message's name-refresh + preview/unread
/// bump cost ONE contacts.json write instead of two (efficiency; the name rule
/// matches `refresh_contact_name`: a generated label never clobbers a real one).
/// Serialized under `contacts_gate` (receive-path RMW; must not be called while
/// the gate is already held).
async fn touch_contact_message_named(
    did: &str,
    name: &str,
    preview: &str,
    ts: i64,
    inc_unread: u32,
) -> Result<(), RuntimeError> {
    // Inbound named messages (the common cross-host receive path) re-open a soft-deleted
    // chat too — must mirror touch_contact_message, else a message that arrives with the
    // sender's nickname after a local delete would never un-hide the thread. Done before
    // the gate (set_chat_hidden touches a separate file, doesn't take contacts_gate).
    set_chat_hidden(did, false).await;
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        c.last_ts = ts;
        c.last_preview = preview.chars().take(140).collect();
        c.unread = c.unread.saturating_add(inc_unread);
        if !name.is_empty()
            && c.name != name
            && !(is_generated_label(name) && !is_generated_label(&c.name))
        {
            c.name = name.to_string();
        }
    } else {
        list.push(DmContact {
            did: did.into(),
            peer_ticket: None,
            ticket_self_asserted: false,
            name: if is_generated_label(name) { String::new() } else { name.to_string() },
            last_ts: ts,
            last_preview: preview.chars().take(140).collect(),
            unread: inc_unread,
            my_inbound_queue: None,
            my_recv_pseudonym: None,
            their_inbound_queue: None,
            my_send_pseudonym: None,
            peer_pubkeys: None,
            key_pop: None,
            status: ContactStatus::Active,
            mode: IdentityMode::Regular,
            anon_identity: None,
            ratchet_capable: false,
            key_verified: true,
            key_changed: false,
            oob_verified: false,
            my_queue_rotated_at: 0,
            my_queue_msg_count: 0,
            retired_queues: Vec::new(),
            salted_queue: None,
            peer_salted: false,
            peer_salted_at: 0,
            salted_self_ready_at: 0,
            needs_verify_before_send: false,
        });
    }
    list.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    write_contacts(&list).await
}

pub async fn mark_read(did: &str) {
    let _g = contacts_gate().lock().await; // serialize contact RMW (race fix)
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        c.unread = 0;
        let _ = write_contacts(&list).await;
    }
}
// (group unread reset already exists as `mark_group_read` below; chat_mark_group_read calls it)

/// Refresh a contact's display name to the sender's CURRENT nickname (carried as
/// "sn" in every DM) so chat shows the live nickname even for a contact who never
/// posted / isn't followed. Only writes on an actual change.
async fn refresh_contact_name(did: &str, name: &str) {
    if name.is_empty() { return; }
    let _g = contacts_gate().lock().await; // serialize contact RMW (race fix)
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        // Don't let a GENERATED label (e.g. "hey-XXXXXX") clobber a real
        // nickname we already hold — but a real name always replaces a
        // placeholder, and any real change is adopted.
        if c.name != name && !(is_generated_label(name) && !is_generated_label(&c.name)) {
            c.name = name.to_string();
            let _ = write_contacts(&list).await;
        }
    }
}

/// F-11: if a received message body carries the salted-support flag (`sc:true`),
/// record that this peer also listens on the salted per-pair topic. Once flipped
/// (and never un-flipped — a one-way upgrade so we don't flap back to the leaky
/// topic), `send_body_to_contact` MIGRATES its sends to the salted topic. We keep
/// listening on the legacy topic regardless, so this can never strand a message.
async fn note_peer_salted(did: &str, body: &Value) {
    if body.get("sc").and_then(Value::as_bool) != Some(true) {
        return;
    }
    let _g = contacts_gate().lock().await; // serialize contact RMW (race fix)
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did && !c.peer_salted) {
        c.peer_salted = true;
        // F-LEGACY-PAIR-TOPIC: stamp WHEN we migrated so my_v2_topics can drop the
        // leaky legacy pair-topic subscription after a bounded grace window.
        c.peer_salted_at = now_ms();
        let _ = write_contacts(&list).await;
    }
}

/// AUTO-PROPAGATE a nickname change. Called right after the social layer writes
/// the new profile: pushes the live name to everyone who'd otherwise wait for my
/// next message.
///   • CHATS (1:1): send a HIDDEN `{ "profile_name": <name> }` control DM to every
///     active v2 contact, so their chat list/header refreshes immediately (the
///     receiver applies it via `refresh_contact_name` and never stores it).
///   • GROUPS: update MY OWN roster entry name locally in every group I'm in (the
///     per-message group "sn" already carries the live name to other members).
/// Best-effort + non-fatal: a contact we can't reach just learns the name on the
/// next normal message (every DM/group msg carries "sn"/"profile_name").
pub async fn broadcast_profile_name(name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    // 1:1 — hidden control DM to each active v2 contact.
    // INCOGNITO LEAK FIX (sec round 1): NEVER ship my real display name to an ANONYMOUS-mode
    // (incognito) contact — that persona is supposed to hide my real DID/profile/name (see
    // shared_display_name, which already suppresses the name at invite/handshake time). Sending the
    // live-nickname control DM here would deanonymize the persona. Only Regular contacts get it.
    let body = json!({ "profile_name": name });
    for c in list_contacts().await {
        if c.is_v2_active() && c.status == ContactStatus::Active && c.mode == IdentityMode::Regular {
            let _ = send_body_to_contact(&c.did, &body).await;
        }
    }
    // GROUPS — refresh my own member entry name in every group (so group_info /
    // the member list show my live nickname locally too). Guarded so we only
    // write_groups on an actual change.
    let me = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if !me.is_empty() {
        let mut groups = read_groups().await;
        let mut changed = false;
        for g in groups.iter_mut() {
            if let Some(m) = g.members.iter_mut().find(|m| m.did == me) {
                if m.name != name {
                    m.name = name.to_string();
                    changed = true;
                }
            }
        }
        if changed {
            let _ = write_groups(&groups).await;
        }
    }
}

/// One-time migration: zero PHANTOM unread left by hidden control messages that
/// bumped the badge before the `is_hidden_ctrl` fix. Those counts can't be cleared
/// by opening a chat (there's nothing visible to read). In-process atomic + an
/// on-disk marker so it runs exactly once, ever; the badge is accurate afterward.
static UNREAD_RESET_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const UNREAD_RESET_MARKER: &str = "dm/unread-reset-v1.json";

async fn reset_phantom_unread_once() {
    use std::sync::atomic::Ordering;
    if UNREAD_RESET_DONE.load(Ordering::Relaxed) {
        return;
    }
    if storage::read_json(UNREAD_RESET_MARKER).await.ok().flatten().is_some() {
        UNREAD_RESET_DONE.store(true, Ordering::Relaxed);
        return;
    }
    let mut list = list_contacts().await;
    if list.iter().any(|c| c.unread != 0) {
        for c in list.iter_mut() {
            c.unread = 0;
        }
        let _ = write_contacts(&list).await;
    }
    let mut groups = read_groups().await;
    if groups.iter().any(|g| g.unread != 0) {
        for g in groups.iter_mut() {
            g.unread = 0;
        }
        let _ = write_groups(&groups).await;
    }
    let _ = storage::write_json(UNREAD_RESET_MARKER, &serde_json::json!(true)).await;
    UNREAD_RESET_DONE.store(true, Ordering::Relaxed);
}

pub async fn total_unread() -> u32 {
    reset_phantom_unread_once().await;
    list_contacts().await.iter().map(|c| c.unread).sum()
}

// ── Expiry (per-contact TTL) ─────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ExpiryMap {
    #[serde(default)]
    map: HashMap<String, i64>,
}

async fn read_expiry() -> ExpiryMap {
    storage::read_json(EXPIRY_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

async fn write_expiry(m: &ExpiryMap) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(m).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(EXPIRY_FILE, &v).await
}

pub async fn get_expiry_secs(did: &str) -> i64 {
    read_expiry().await.map.get(did).copied().unwrap_or(0)
}

pub async fn set_expiry_secs(did: &str, secs: i64) -> Result<(), RuntimeError> {
    let mut m = read_expiry().await;
    if secs <= 0 {
        m.map.remove(did);
    } else {
        m.map.insert(did.into(), secs);
    }
    write_expiry(&m).await
}

pub async fn prune_expired(did: &str) {
    let ttl = get_expiry_secs(did).await;
    if ttl <= 0 {
        return;
    }
    let cutoff = now_ms() - ttl * 1000;
    let conv = read_conversation(did).await;
    if conv.iter().any(|m| m.ts < cutoff) {
        let kept: Vec<DmMessage> = conv.into_iter().filter(|m| m.ts >= cutoff).collect();
        let _ = write_conversation(did, &kept).await;
    }
}

// ── Legacy peer-keys cache (read-only for migration) ────────────────

async fn read_peer_keys() -> HashMap<String, PeerKeys> {
    storage::read_json(PEER_KEYS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

async fn write_peer_keys(map: &HashMap<String, PeerKeys>) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(map).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(PEER_KEYS_FILE, &v).await
}

pub async fn cache_peer_keys(did: &str, keys: PeerKeys) {
    let mut map = read_peer_keys().await;
    map.insert(did.into(), keys);
    let _ = write_peer_keys(&map).await;
}

pub async fn get_peer_keys(did: &str) -> Option<PeerKeys> {
    read_peer_keys().await.get(did).cloned()
}

// ── Key material helpers ─────────────────────────────────────────────

/// Our advertised pubkeys, fetched from the runtime identity provider (the
/// wallet model — private keys never leave it). Used when minting
/// invites/handshakes.
/// My own X25519 + ML-KEM DM pubkeys (from the identity provider). Public so
/// hey-social's hey-friend link can carry them — letting a follow bootstrap a
/// DM-capable contact (unified Following = people you can message).
pub async fn my_pubkeys() -> Option<PeerKeys> {
    let resp = crate::runtime::identity_provider::pubkeys(IDENTITY_NS)
        .await
        .ok()?;
    let d = resp.get("data").unwrap_or(&resp);
    Some(PeerKeys {
        x25519_pub_b64: d.get("x25519_pub_b64")?.as_str()?.to_string(),
        ml_kem_pub_b64: d.get("ml_kem_pub_b64")?.as_str()?.to_string(),
    })
}

/// Adopt the runtime-projected identity with NO passkey tap — the wallet
/// model. Calls identity/whoami; on success installs a PROVIDER-BACKED session
/// (real did:key, EMPTY local seed → every signing + decryption routes through
/// the runtime identity provider). Returns the did, or None if the provider
/// isn't available (the caller then falls back to the passkey ceremony, so
/// removing the fork patch still leaves a working app).
pub async fn adopt_provider_identity() -> Option<String> {
    let resp = crate::runtime::identity_provider::whoami(IDENTITY_NS)
        .await
        .ok()?;
    let d = resp.get("data").unwrap_or(&resp);
    let did = d.get("did_key")?.as_str()?.to_string();
    if !did.starts_with("did:key:z") {
        return None;
    }
    let name = session::current()
        .map(|s| s.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| short_did_label(&did));
    session::set(&session::Session {
        auth_key_hex: String::new(),
        did_key: did.clone(),
        name,
        ml_kem_secret_b64: String::new(),
        ml_kem_public_b64: String::new(),
    });
    Some(did)
}

fn short_did_label(did: &str) -> String {
    if did.len() > 12 {
        format!("hey-{}", &did[did.len() - 6..])
    } else {
        did.to_string()
    }
}

/// True when `name` is a PLACEHOLDER rather than a chosen nickname: empty, a
/// `pending:` stub, or the generated `hey-XXXXXX` short-DID label shape (the
/// `short_did_label` fallback). A real handshake/profile name should always beat
/// one of these — so the name gates use this instead of only `is_empty()`.
pub fn is_generated_label(name: &str) -> bool {
    let n = name.trim();
    if n.is_empty() || n.starts_with("pending:") {
        return true;
    }
    // "hey-" + exactly 6 chars (the short_did_label of a real did:key tail).
    if let Some(tail) = n.strip_prefix("hey-") {
        return tail.len() == 6 && tail.chars().all(|c| c.is_ascii_alphanumeric());
    }
    false
}

// ── Per-contact identity (Regular vs Anonymous) ──────────────────────
//
// In Anonymous mode every outgoing artifact we put on the wire for a
// contact — the invite/handshake `did` + `pubkeys` + `name`, and the
// inner-payload `sender_did` + signature — comes from a fresh ephemeral
// identity instead of the session. The sealed-sender envelope already
// carries no sender key (see crypto::encrypt_to_hybrid), so swapping the
// signing key + advertised pubkeys is all it takes to make us unlinkable.

/// Parse a 64-hex-char string into a 32-byte seed.
fn seed32(hex: &str) -> Result<[u8; 32], String> {
    let v = hex_to_bytes(hex)?;
    if v.len() != 32 {
        return Err("seed must be 32 bytes".into());
    }
    let mut s = [0u8; 32];
    s.copy_from_slice(&v);
    Ok(s)
}

// (mint_anon_identity / anon_pubkeys / anon_user_keys removed with incognito.)

/// The (did, signing-seed-hex) we present to a contact: the session
/// identity in Regular mode, the ephemeral identity in Anonymous mode.
fn signing_identity(
    _mode: IdentityMode,
    _anon: Option<&AnonIdentity>,
    me_did: &str,
    me_auth_key_hex: &str,
) -> Result<(String, String), String> {
    // Incognito/Anonymous removed — always sign as the real session identity.
    Ok((me_did.to_string(), me_auth_key_hex.to_string()))
}

/// The pubkeys we advertise to a contact (real session pubkeys in Regular,
/// ephemeral pubkeys in Anonymous).
fn advertised_pubkeys(
    _mode: IdentityMode,
    _anon: Option<&AnonIdentity>,
    me_pub: &PeerKeys,
) -> Result<PeerKeys, String> {
    // Incognito/Anonymous removed — always advertise the real session pubkeys.
    Ok(me_pub.clone())
}

/// The display name we SHARE with a contact: our real profile name in
/// Regular mode, nothing in Anonymous mode (sharing it would defeat the
/// anonymity — the peer would learn who we are).
fn shared_display_name(_mode: IdentityMode, real_name: &str) -> String {
    // Incognito/Anonymous removed — always share the real name.
    real_name.to_string()
}

/// How to open incoming traffic: with local key material (the session seed,
/// or a per-contact anonymous ephemeral key), or via the runtime identity
/// provider (a provider-backed session has a did:key but no local seed).
enum DecryptVia {
    Local(UserKeys),
    Provider,
}

/// The decrypt path for a specific contact. Anonymous contacts ALWAYS decrypt
/// locally with their per-contact ephemeral key — never the provider, which
/// does not hold it (must-fix #3). For the regular identity: a provider-backed
/// session (empty seed) decrypts via the runtime; otherwise the local seed.
fn decrypt_via_for_contact(_c: &DmContact) -> Result<DecryptVia, String> {
    // Incognito/Anonymous removed — every contact decrypts via the session identity.
    decrypt_via_for_session()
}

/// Decrypt path for the session identity itself (no per-contact override).
/// Wallet-only: the runtime identity provider holds the key, so this is
/// always the provider path.
fn decrypt_via_for_session() -> Result<DecryptVia, String> {
    Ok(DecryptVia::Provider)
}

/// Choose the decrypt path for traffic arriving on `queue_id`. An unknown queue
/// (self-test/legacy) falls back to the session's path.
async fn decrypt_via_for_queue(queue_id: Option<&str>) -> Result<DecryptVia, String> {
    if let Some(qid) = queue_id {
        if let Some(c) = list_contacts()
            .await
            .into_iter()
            .find(|c| c.my_inbound_queue.as_deref() == Some(qid))
        {
            return decrypt_via_for_contact(&c);
        }
    }
    decrypt_via_for_session()
}

/// Compute the two hybrid shared secrets (X25519 ECDH output, ML-KEM
/// decapsulated secret) for an `(eph_pub, kem_ct)` pair — locally from our key
/// material, or via the identity provider (private keys never leave it). This
/// is the recipient half of both the single-shot decrypt AND the ratchet
/// bootstrap's SK recovery.
async fn shared_secrets(
    via: &DecryptVia,
    eph_pub: &[u8],
    kem_ct: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    match via {
        DecryptVia::Local(keys) => {
            let eph: [u8; 32] = eph_pub
                .try_into()
                .map_err(|_| "eph wrong size".to_string())?;
            let x = crypto::dh(&keys.x25519_priv, &eph);
            let k = crypto::ml_kem_decapsulate_local(kem_ct, &keys.ml_kem_secret_bytes)?;
            Ok((x.to_vec(), k))
        }
        DecryptVia::Provider => {
            let x = crate::runtime::identity_provider::x25519_dh(IDENTITY_NS, eph_pub)
                .await
                .map_err(|e| format!("provider x25519_dh: {e}"))?;
            let k = crate::runtime::identity_provider::ml_kem_decapsulate(IDENTITY_NS, kem_ct)
                .await
                .map_err(|e| format!("provider ml_kem_decapsulate: {e}"))?;
            let x_shared =
                crate::runtime::identity_provider::shared_from(&x).map_err(|e| e.to_string())?;
            let k_shared =
                crate::runtime::identity_provider::shared_from(&k).map_err(|e| e.to_string())?;
            Ok((x_shared, k_shared))
        }
    }
}

/// Open one single-shot sealed envelope to plaintext (X25519-static + ML-KEM
/// hybrid). Ratchet messages instead supply the X25519-half as the chain
/// message key and only need the KEM-half (`ratchet_kem_ss`).
async fn open_envelope(env: &HpqEnvelope, via: &DecryptVia) -> Result<String, String> {
    let (eph, kem_ct) = crypto::envelope_recipient_inputs(env)?;
    let (x, k) = shared_secrets(via, &eph, &kem_ct).await?;
    crypto::open_with_secrets(env, &x, &k)
}

/// The ML-KEM shared secret for a ratchet envelope (its KEM-half). The X25519
/// half of a ratchet message is the chain message key `mk`, NOT an ECDH against
/// a static key, so we only decapsulate `env.kem` here. Anon ⇒ local anon key;
/// provider-backed ⇒ runtime; else local seed.
async fn ratchet_kem_ss(env: &HpqEnvelope, via: &DecryptVia) -> Result<Vec<u8>, String> {
    let (_eph, kem_ct) = crypto::envelope_recipient_inputs(env)?;
    match via {
        DecryptVia::Local(keys) => {
            crypto::ml_kem_decapsulate_local(&kem_ct, &keys.ml_kem_secret_bytes)
        }
        DecryptVia::Provider => {
            let k = crate::runtime::identity_provider::ml_kem_decapsulate(IDENTITY_NS, &kem_ct)
                .await
                .map_err(|e| format!("provider ml_kem_decapsulate: {e}"))?;
            crate::runtime::identity_provider::shared_from(&k).map_err(|e| e.to_string())
        }
    }
}

// ── Invite link codec ────────────────────────────────────────────────
//
// An invite link is the OOB introduction. Alice generates one for each
// new contact, sends it through any channel (QR, email, Signal, IRL),
// and the recipient pastes it to bootstrap a metadata-safe DM channel.
//
// Link payload (base64url-encoded JSON, no padding):
//   {
//     "v":     1,
//     "queue": "<256bit hex>",      ← Alice's inbound queue
//     "did":   "did:key:z...",      ← Alice's identity (sig verification)
//     "name":  "Alice",
//     "keys":  { "x25519_pub_b64", "ml_kem_pub_b64" },
//     "nonce": "<128bit hex>"       ← per-link random, opaque
//   }
//
// The DID is in the link because (a) it's an OOB channel, by definition
// shared in confidence, and (b) the recipient needs it to verify the
// inner Ed25519 signature on Alice's first encrypted reply. The link is
// never sent over the runtime — once consumed, it's destroyed.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteLink {
    pub v: u8,
    pub queue: String,
    pub did: String,
    #[serde(default)]
    pub name: String,
    pub keys: PeerKeys,
    pub nonce: String,
    /// Unix-ms expiry. `decode_invite_link` refuses tokens past this.
    /// Older v1 links omit it; for v=1 we treat as "no expiry."
    #[serde(default)]
    pub expires_at: i64,
    /// The inviter's ratchet prekey (Double Ratchet bootstrap). Additive +
    /// optional: an invite WITHOUT it (old link, or a peer that doesn't
    /// ratchet) negotiates the single-shot path. Present ⇒ the accepter can
    /// bootstrap a ratchet and signals it back in the handshake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratchet: Option<RatchetPrekey>,
    /// The inviter's peer node ticket (iroh EndpointId). The accepter passes it
    /// as the gossip bootstrap so its runtime forms the mesh directly with the
    /// inviter's runtime — this is what makes cross-runtime delivery work with
    /// no central hub. Additive + optional: a link without it (older link, or a
    /// same-runtime-only node) falls back to a bootstrap-less join.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_ticket: Option<String>,
    /// Ed25519 signature (hex) by `did` over the canonical invite (this struct
    /// with `sig` cleared). Proof the inviter owns the did:key it advertises, so
    /// an in-path attacker can't substitute the keys. REQUIRED on decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

/// Sign an invite with the inviter's Ed25519 seed — proof-of-possession of the
/// advertised did:key, so the keys inside can't be swapped by an in-path
/// attacker. Sets `invite.sig` (signs the struct with `sig` cleared).
fn sign_invite(invite: &mut InviteLink, seed: &[u8; 32]) {
    invite.sig = None;
    let canonical = serde_json::to_vec(&invite).unwrap_or_default();
    invite.sig = Some(sign(&canonical, seed));
}

pub fn encode_invite_link(invite: &InviteLink) -> String {
    let j = serde_json::to_vec(invite).unwrap_or_default();
    B64URL.encode(&j)
}

/// Render an invite link as a scannable QR-code SVG string. Used by
/// the chat UI to offer a "show QR" alternative to copy-paste. Returns
/// None if the link is too long for a QR code (very unlikely; v2 links
/// fit comfortably in version 27 ≈ 1500 bytes).
pub fn invite_qr_svg(token: &str) -> Option<String> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};
    // EcLevel::L (not M) + a larger min size: the device-link payload is ~1 KB
    // (heyapp:// + the dl1. envelope), which lands around QR version 25. At the
    // old 220px that was ~1.9px/module — below the ~2.5px a phone camera needs,
    // so the QR wouldn't scan. L drops a few versions and 360px restores
    // ~3px/module. Harmless for the shorter DID / invite QRs (they just render
    // at a low version, big and crisp).
    let code = QrCode::with_error_correction_level(token.as_bytes(), EcLevel::L).ok()?;
    Some(
        code.render::<svg::Color<'_>>()
            .min_dimensions(360, 360)
            .dark_color(svg::Color("#0a0a0a"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

pub fn decode_invite_link(token: &str) -> Result<InviteLink, String> {
    // Tolerate users pasting "hey-invite:" prefix or whitespace/newlines.
    let stripped = token.trim();
    let stripped = stripped
        .strip_prefix("hey-invite:")
        .unwrap_or(stripped)
        .trim();
    let bytes = B64URL
        .decode(stripped)
        .map_err(|e| format!("invite base64: {e}"))?;
    let mut invite: InviteLink =
        serde_json::from_slice(&bytes).map_err(|e| format!("invite json: {e}"))?;
    // We currently emit v=2 (with expires_at). Accept v=1 too so old
    // links keep working — they don't have expiry but every other
    // field is identical.
    if invite.v != INVITE_LINK_VERSION && invite.v != 1 {
        return Err(format!(
            "unsupported invite link version {} (expected 1 or {INVITE_LINK_VERSION})",
            invite.v
        ));
    }
    if !invite.did.starts_with("did:key:z") {
        return Err("invite did is not a did:key".into());
    }
    if invite.queue.len() != 64 {
        return Err("invite queue is not 256-bit hex".into());
    }
    if invite.expires_at > 0 && invite.expires_at < now_ms() {
        return Err("invite link has expired — ask for a fresh one".into());
    }
    // Proof-of-possession: verify the inviter's Ed25519 signature over the
    // canonical invite (this struct with `sig` cleared) against the pubkey
    // EMBEDDED in its did:key. REQUIRED — an unsigned or tampered invite is
    // rejected, so an in-path attacker cannot substitute the advertised keys.
    let sig = invite.sig.take().ok_or(
        "invite is not signed — ask the sender for a fresh invite (their app needs updating)",
    )?;
    let canonical = serde_json::to_vec(&invite).map_err(|e| format!("invite canonical: {e}"))?;
    let pk = did_key_to_public_key(&invite.did)?;
    if !verify(&canonical, &sig, &pk) {
        return Err("invite signature did not verify — the keys may have been tampered with".into());
    }
    invite.sig = Some(sig);
    Ok(invite)
}

/// Mint a fresh invite for an unknown contact. The recipient's DID
/// isn't required at generation time — it's recovered from the inner
/// signature on their handshake reply. The contact is stashed under a
/// placeholder DID (`pending:<queue>`) until the handshake lands.
///
/// `display_label` is what we want to see in our own contact list for
/// this pending invite (e.g. "Bob from work"). Cosmetic; the real name
/// is overwritten by the handshake body if the peer sends one.
pub async fn generate_invite(display_label: &str, mode: IdentityMode, anon_name: &str) -> Result<String, String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let my_pub = my_pubkeys()
        .await
        .ok_or_else(|| "no pubkeys (not signed in)".to_string())?;

    // Incognito/Anonymous removed — always present the real session identity.
    let anon: Option<AnonIdentity> = None;
    let share_did = me.did_key.clone();
    let share_pub = advertised_pubkeys(mode, anon.as_ref(), &my_pub)?;
    // The seed behind the ADVERTISED did (anon or regular) — captured here before
    // `anon` is moved into the contact record; used to sign the invite below so
    // its signature verifies against `invite.did`'s embedded pubkey on decode.
    let sign_seed = {
        let s = session::current().ok_or_else(|| "not signed in".to_string())?;
        let (_d, seed_hex) = signing_identity(mode, anon.as_ref(), &me.did_key, &s.auth_key_hex)?;
        seed32(&seed_hex)?
    };
    // The name advertised TO THE PEER: always our real profile name (incognito removed).
    let _ = anon_name;
    let share_name = me.name.clone();

    let queue = random_hex(32);
    let recv_pseudonym = random_hex(16);
    let send_pseudonym = random_hex(16);
    let nonce = random_hex(16);

    // Ratchet prekey: a fresh DH keypair we publish in the invite. The accepter
    // uses its public half to bootstrap; we keep the private half stashed (as a
    // prekey-only ratchet state) until the handshake completes the bootstrap.
    let (prekey_priv, prekey_pub) = crypto::ratchet_keypair();

    // Placeholder DID until the handshake reply arrives and gives us
    // the real one. We disambiguate pending invites by queue id.
    let placeholder_did = format!("pending:{queue}");
    let contact = DmContact {
        did: placeholder_did.clone(),
        peer_ticket: None,
        ticket_self_asserted: false,
        name: display_label.trim().to_string(),
        last_ts: now_ms(),
        last_preview: String::from("Invite sent — awaiting reply"),
        unread: 0,
        my_inbound_queue: Some(queue.clone()),
        my_recv_pseudonym: Some(recv_pseudonym),
        their_inbound_queue: None,
        my_send_pseudonym: Some(send_pseudonym),
        peer_pubkeys: None,
        key_pop: None,
        status: ContactStatus::PendingInvite,
        mode,
        anon_identity: anon,
        ratchet_capable: false,
        key_verified: true,
        key_changed: false,
        oob_verified: false,
        my_queue_rotated_at: 0,
        my_queue_msg_count: 0,
        retired_queues: Vec::new(),
        salted_queue: None,
        peer_salted: false,
        peer_salted_at: 0,
        salted_self_ready_at: 0,
        needs_verify_before_send: false,
    };
    upsert_contact_record(contact)
        .await
        .map_err(|e| e.to_string())?;

    // Stash the prekey privkey as a not-yet-bootstrapped ratchet state under the
    // placeholder DID. receive_handshake reads it to complete the bootstrap.
    let prekey_state = RatchetState {
        rk: String::new(),
        cks: None,
        ckr: None,
        dhs_priv: hx(&prekey_priv),
        dhs_pub: hx(&prekey_pub),
        dhr_pub: None,
        ns: 0,
        nr: 0,
        pn: 0,
        skipped: Vec::new(),
        // The inviter's rolling KEM keypair is minted at bootstrap
        // (ratchet_init_responder), not here — this is just the DH prekey stash.
        kem_priv: None,
        kem_pub: None,
        peer_kem_pub: None,
        send_kem_ct: None,
    };
    let _ = write_ratchet(&placeholder_did, &prekey_state).await;

    // Join our new inbound queue topic so the peer_receiver picks up
    // their handshake reply.
    let _ = peer::join_topic(&format!("{TOPIC_PREFIX_V2}/{queue}")).await;

    let mut invite = InviteLink {
        v: INVITE_LINK_VERSION,
        queue,
        did: share_did,
        name: share_name,
        keys: share_pub,
        nonce,
        expires_at: now_ms() + INVITE_TTL_MS,
        ratchet: Some(RatchetPrekey {
            dh_pub_b64: B64.encode(prekey_pub),
        }),
        // Carry our node ticket so the accepter's runtime can bootstrap the
        // gossip mesh straight to ours (cross-runtime, no hub). COMPACT it (mirrors
        // the `nt` field on outgoing DMs) so the shareable link never ships our FULL
        // direct-IP set — relays + a bounded set of same-LAN dial hints are kept.
        node_ticket: peer::my_ticket().await.map(|t| compact_nt_ticket(&t)),
        sig: None,
    };
    sign_invite(&mut invite, &sign_seed);
    Ok(format!("hey-invite:{}", encode_invite_link(&invite)))
}

/// Revoke a pending OUTGOING invite that hasn't been accepted yet.
///
/// `id` may be either the placeholder DID the UI shows for the contact
/// (`pending:<queue>`) or the raw inbound-queue id — both resolve to the same
/// record. Revoking:
///   1. drops the local `PendingInvite` contact record,
///   2. leaves the invite's gossip queue so a LATE handshake is rejected —
///      `receive_handshake` can no longer find a `PendingInvite` on that queue,
///      so it silently drops the reply (a leaked link becomes inert),
///   3. clears the stashed (not-yet-bootstrapped) ratchet prekey.
///
/// Idempotent: revoking something already gone (double-tap, or it got accepted
/// in the meantime) returns `Ok(())`. Only ever touches `PendingInvite`
/// contacts — an `Active` conversation is never removed here.
pub async fn revoke_invite(id: &str) -> Result<(), String> {
    // Serialize against the receive-path contacts RMW (same reason as delete_conversation).
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let Some(pos) = list.iter().position(|c| {
        c.status == ContactStatus::PendingInvite
            && (c.did == id || c.my_inbound_queue.as_deref() == Some(id))
    }) else {
        return Ok(());
    };
    let removed = list.remove(pos);
    write_contacts(&list).await.map_err(|e| e.to_string())?;

    // Stop listening on the invite queue so a late handshake can't relight it.
    if let Some(queue) = removed.my_inbound_queue.as_deref() {
        crate::peer_receiver::forget_topic(&format!("{TOPIC_PREFIX_V2}/{queue}")).await;
    }
    // Clear the not-yet-bootstrapped ratchet prekey stash (keyed by placeholder DID).
    remove_ratchet(&removed.did).await;
    Ok(())
}

/// Delete an EXISTING conversation for THIS device — the Active-contact
/// counterpart to `revoke_invite`. Drops the contact record, the message log,
/// and the ratchet state, and stops listening on every queue the contact owned
/// (minted inbound queue, the deterministic per-pair queue for Regular
/// contacts, and any retired queues) plus purges its outbox backlog.
///
/// Local-only: the peer is NOT notified and keeps their side. A later message
/// from them lands on a queue we no longer own and is dropped as "unowned"
/// (no silent auto-recreate) — re-add them with a fresh invite to resume.
/// Idempotent: deleting something already gone returns `Ok(())`.
// ── Soft "Delete chat": wipe all LOCAL data for a conversation + hide it, but KEEP the
//    contact + its subscribed queues so a future message from them re-opens the chat (delete is
//    NOT a block). "Hidden" lives in its own small file so the engine still receives + decrypts
//    their messages; the UI just doesn't list the chat until it has content again. ──────────────
const HIDDEN_CHATS_PATH: &str = "dm/hidden-chats.json";

async fn hidden_chats() -> std::collections::HashSet<String> {
    storage::read_json(HIDDEN_CHATS_PATH)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// The set of soft-deleted (hidden) chats — the UI filters these out of the chat list.
pub async fn hidden_chat_set() -> std::collections::HashSet<String> {
    hidden_chats().await
}

async fn set_chat_hidden(did: &str, hidden: bool) {
    let mut set = hidden_chats().await;
    let changed = if hidden { set.insert(did.to_string()) } else { set.remove(did) };
    if changed {
        let v = serde_json::to_value(set.into_iter().collect::<Vec<_>>())
            .unwrap_or_else(|_| serde_json::json!([]));
        let _ = storage::write_json(HIDDEN_CHATS_PATH, &v).await;
    }
}

/// Re-show a soft-deleted chat (any new message, or an explicit re-add, un-hides it).
pub async fn unhide_chat(did: &str) {
    set_chat_hidden(did, false).await;
}

// ── CHAT-CAPABILITY ISOLATION ─────────────────────────────────────────────────
// A FOLLOW relationship must grant ZERO chat capability: accepting/being a follower bootstraps a
// DM contact ONLY to carry the (one-way, SOH-control) feed key, and that must never become a usable
// private chat. Chat is allowed for a contact ONLY when it was EXPLICITLY established via a chat
// QR / invite (chat_only link, hey-invite, or an inbound chat_only announce) — recorded in this set.
// Membership is SET-ONLY (NOT derived from last_ts): a raw last_ts>0 grandfather was exploitable —
// an inbound user text from a non-conforming/hostile peer bumps last_ts and would silently flip a
// follow-only contact to chat-enabled. Instead, pre-existing real chats are seeded into the set ONCE
// by migrate_chat_enabled_from_history() at boot, so an inbound message can never promote a contact.
const CHAT_ENABLED_PATH: &str = "dm/chat-enabled.json";
const CHAT_ENABLED_MIGRATED_PATH: &str = "dm/chat-enabled-migrated.json";

async fn chat_enabled_set() -> std::collections::HashSet<String> {
    storage::read_json(CHAT_ENABLED_PATH)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Record that `did` may be chatted with (an explicit chat QR / invite / chat_only announce). Once
/// set it persists — a later follow re-pair never revokes a real chat. Idempotent. Also RESURFACES
/// any messages that arrived BEFORE chat was enabled (the receive gate buffers a user message from a
/// not-yet-enabled contact without surfacing it; enabling chat then makes the thread + its buffered
/// messages appear, so a legitimate first message racing ahead of the chat_only announce is shown,
/// never lost).
pub async fn enable_chat(did: &str) {
    let mut set = chat_enabled_set().await;
    if set.insert(did.to_string()) {
        let v = serde_json::to_value(set.into_iter().collect::<Vec<_>>())
            .unwrap_or_else(|_| serde_json::json!([]));
        let _ = storage::write_json(CHAT_ENABLED_PATH, &v).await;
    }
    // Resurface any buffered (received-while-disabled) conversation: if there is history but the
    // contact never surfaced (last_ts==0), touch it with the latest message so the thread appears.
    let conv = read_conversation(did).await;
    if let Some(last) = conv.iter().filter(|m| !m.text.starts_with('\u{1}')).last() {
        let already = list_contacts().await.iter().any(|c| c.did == did && c.last_ts > 0);
        if !already {
            let preview = if last.text.is_empty() && !last.attachments.is_empty() {
                format!("📎 {}", last.attachments[0].name)
            } else {
                last.text.clone()
            };
            let _ = touch_contact_message(did, &preview, last.ts, if last.mine { 0 } else { 1 }).await;
        }
    }
}

/// REVOKE chat capability for `did` — remove it from the chat-enabled set. Called when the user
/// deletes the chat: under FOLLOW≠CHAT, a deleted chat severs the chat relationship, so messaging
/// (or being messaged) requires re-establishing via a chat QR / invite. Idempotent.
pub async fn disable_chat(did: &str) {
    let mut set = chat_enabled_set().await;
    if set.remove(did) {
        let v = serde_json::to_value(set.into_iter().collect::<Vec<_>>())
            .unwrap_or_else(|_| serde_json::json!([]));
        let _ = storage::write_json(CHAT_ENABLED_PATH, &v).await;
    }
}

/// One-time migration: seed the chat-enabled set from every contact that ALREADY has real history
/// (last_ts > 0) at upgrade time. This preserves every pre-existing private chat under the new
/// set-only rule (so they remain sendable), WITHOUT the exploitable live last_ts grandfather. A
/// soft delete keeps the entry (recoverable); only BLOCK revokes it. Marker-guarded ⇒ safe to call
/// on every boot; only the first run does work.
pub async fn migrate_chat_enabled_from_history() {
    let done = storage::read_json(CHAT_ENABLED_MIGRATED_PATH)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if done {
        return;
    }
    let mut set = chat_enabled_set().await;
    let mut changed = false;
    for c in list_contacts().await {
        if c.last_ts > 0 && set.insert(c.did.clone()) {
            changed = true;
        }
    }
    if changed {
        let v = serde_json::to_value(set.into_iter().collect::<Vec<_>>())
            .unwrap_or_else(|_| serde_json::json!([]));
        let _ = storage::write_json(CHAT_ENABLED_PATH, &v).await;
    }
    let _ = storage::write_json(CHAT_ENABLED_MIGRATED_PATH, &serde_json::json!(true)).await;
}

/// True iff a private chat with `did` is permitted: chat was EXPLICITLY established (present in the
/// set — via a chat QR / invite / chat_only announce, or seeded from pre-existing history by the
/// boot migration). A follow-only contact is FALSE → the chat send + receive paths fail closed, so
/// following someone can never open a chat. SET-ONLY by design (see the module comment): an inbound
/// message bumping last_ts must NOT be able to promote a follow-only contact.
pub async fn is_chat_enabled(did: &str) -> bool {
    chat_enabled_set().await.contains(did)
}

/// FEED-side helper (follow / accept_follower): a contact bootstrapped purely to deliver the feed
/// key should NOT surface as an empty "Tap to chat" row — keep the chat list to REAL conversations.
/// Hide it ONLY while it has no real message yet (`last_ts == 0`; feed-key/handshake control DMs are
/// SOH-prefixed and never bump last_ts, so a control-only contact stays 0) AND it is NOT chat-enabled.
/// The chat-enabled guard is critical: if you established a chat (chat QR/invite) but haven't sent a
/// message yet, then a FOLLOW from the same person must NOT hide that real (empty) chat — that was a
/// bug where an inbound follow made a just-accepted chat vanish from the list. The first real message
/// in either direction un-hides it (touch_contact_message), and a contact with history (`last_ts > 0`)
/// is left visible. Best-effort; the last_ts guard also avoids hiding a chat that just got a message.
pub async fn hide_chat_if_empty(did: &str) {
    // NEVER hide a chat-enabled contact — it's a real chat (even if message-less), not a follow row.
    if is_chat_enabled(did).await {
        return;
    }
    let last_ts = list_contacts()
        .await
        .iter()
        .find(|c| c.did == did)
        .map(|c| c.last_ts)
        .unwrap_or(0);
    if last_ts == 0 {
        set_chat_hidden(did, true).await;
    }
}

/// SOFT delete: wipe the conversation history + reset its counters + HIDE it, but KEEP the contact
/// and its subscribed queues. A future inbound message (touch_contact_message) un-hides it, so the
/// chat re-opens with the new message — exactly "delete the local data; they can still message me."
pub async fn hide_conversation(did: &str) -> Result<(), String> {
    // Unpin this conversation's attachment blobs (best-effort) BEFORE wiping the log that references
    // their CIDs — a soft delete still removes the local history, so without this the encrypted
    // attachment files would linger pinned in the content store with nothing pointing at them.
    for m in read_conversation(did).await {
        for att in &m.attachments {
            if !att.cid.is_empty() {
                let _ = crate::runtime::content::unpin(&att.cid).await;
            }
        }
    }
    let _ = storage::remove(&conv_path(did)).await; // wipe local history
    {
        let _g = contacts_gate().lock().await;
        let mut list = list_contacts().await;
        if let Some(c) = list.iter_mut().find(|c| c.did == did) {
            c.last_ts = 0;
            c.last_preview = String::new();
            c.unread = 0;
            let _ = write_contacts(&list).await;
        }
    }
    set_chat_hidden(did, true).await;
    // SOFT delete (recoverable): we KEEP chat capability (the contact stays chat-enabled). So an
    // ACCIDENTAL delete isn't destructive — the other side can still message us and the chat
    // reappears as a new DM (the receive path un-hides + surfaces it), and we can re-open it
    // ourselves. Deliberately severing a chat is BLOCK (block_follower → disable_chat), not delete.
    Ok(())
}

pub async fn delete_conversation(did: &str) -> Result<(), String> {
    // Serialize the contacts.json read-modify-write against the receive-path RMW (handshake/
    // welcome/queue-rotation/touch) — an unlocked RMW here races them and, with the now-atomic but
    // last-writer-wins file write, would drop a concurrent update.
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let Some(pos) = list.iter().position(|c| c.did == did) else {
        return Ok(());
    };
    let removed = list.remove(pos);
    write_contacts(&list).await.map_err(|e| e.to_string())?;

    // Stop listening on + retrying every queue this contact routed over.
    let my_did = ensure_profile().await.map(|m| m.did_key).ok();
    let mut topics: Vec<String> = Vec::new();
    if let Some(q) = removed.my_inbound_queue.as_deref() {
        topics.push(format!("{TOPIC_PREFIX_V2}/{q}"));
    }
    if matches!(removed.mode, IdentityMode::Regular) {
        if let Some(md) = &my_did {
            topics.push(format!(
                "{TOPIC_PREFIX_V2}/{}",
                pair_inbound_queue(md, &removed.did)
            ));
        }
    }
    // F-11: also stop listening on the salted per-pair topic (if one was pinned).
    if let Some(q) = removed.salted_queue.as_deref() {
        topics.push(format!("{TOPIC_PREFIX_V2}/{q}"));
    }
    for rq in &removed.retired_queues {
        topics.push(format!("{TOPIC_PREFIX_V2}/{}", rq.queue));
    }
    for t in &topics {
        crate::peer_receiver::forget_topic(t).await;
        crate::api::outbox::purge_topic(t).await;
    }

    // Unpin this conversation's attachment blobs (best-effort) BEFORE dropping
    // the log that references their CIDs — otherwise the encrypted files linger
    // pinned in the content store with nothing pointing at them.
    for m in read_conversation(&removed.did).await {
        for att in &m.attachments {
            if !att.cid.is_empty() {
                let _ = crate::runtime::content::unpin(&att.cid).await;
            }
        }
    }

    // Drop the message log + ratchet state (+ any leftover pending prekey stash).
    let _ = storage::remove(&conv_path(&removed.did)).await;
    remove_ratchet(&removed.did).await;
    if let Some(q) = removed.my_inbound_queue.as_deref() {
        remove_ratchet(&format!("pending:{q}")).await;
    }

    // Scrub this peer's entries from the shared per-DID maps (cached pubkeys +
    // disappearing-message expiry) so no trace of the contact survives.
    let mut pk = read_peer_keys().await;
    if pk.remove(&removed.did).is_some() {
        let _ = write_peer_keys(&pk).await;
    }
    let mut ex = read_expiry().await;
    if ex.map.remove(&removed.did).is_some() {
        let _ = write_expiry(&ex).await;
    }
    Ok(())
}

/// Accept someone else's invite link. Creates an Active contact, sends
/// the handshake reply (encrypted to their pubkeys) to their queue, and
/// returns the contact's DID so the UI can navigate to the conversation.
///
/// Idempotent on double-click / re-paste: if we already have an Active
/// contact with this DID + pubkeys, we just return its DID without
/// minting a new queue or re-publishing a handshake. Avoids the
/// double-handshake deadlock where Bob's second click would point him
/// at a queue Alice never learns about.
pub async fn accept_invite(token: &str, mode: IdentityMode) -> Result<String, String> {
    let invite = decode_invite_link(token)?;
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    if invite.did == me.did_key {
        return Err("that's your own invite link".into());
    }
    // CHAT-CAPABILITY: accepting a chat invite is explicit consent → permit a private chat (covers
    // both the idempotent re-accept and the fresh bootstrap below).
    enable_chat(&invite.did).await;
    if let Some(existing) = find_contact(&invite.did).await {
        if existing.status == ContactStatus::Active && existing.peer_pubkeys.is_some() {
            // Idempotent re-accept: back-fill any missing queues/pseudonyms so a
            // partially-torn-down (e.g. delete-chat'd) contact is fully wired again.
            // lift_hidden=true: re-accepting an invite is genuine user re-engagement.
            repair_contact(&existing.did, true).await;
            return Ok(existing.did);
        }
    }
    let s = session::current().ok_or_else(|| "not signed in".to_string())?;
    let my_pub = my_pubkeys()
        .await
        .ok_or_else(|| "no pubkeys (not signed in)".to_string())?;

    // Incognito/Anonymous removed — always present the real session identity.
    let anon: Option<AnonIdentity> = None;
    let (my_did, my_seed_hex) =
        signing_identity(mode, anon.as_ref(), &me.did_key, &s.auth_key_hex)?;
    let share_pub = advertised_pubkeys(mode, anon.as_ref(), &my_pub)?;
    let share_name = shared_display_name(mode, &me.name);

    // Mint OUR queue for receiving from them.
    let my_queue = random_hex(32);
    let my_recv_pseudonym = random_hex(16);
    let my_send_pseudonym = random_hex(16);

    // Ratchet bootstrap (we are the INITIATOR). Only if the invite advertised a
    // prekey — otherwise we negotiate the single-shot path with this peer.
    // SK is derived from a FRESH bootstrap ephemeral (discarded after — must-fix
    // #5) DH'd against the inviter's advertised static X25519, plus an ML-KEM
    // encapsulation to their advertised KEM key. All local even when we are
    // provider-backed (encap needs only their public key).
    let ratchet_bootstrap: Option<(RatchetBootstrap, RatchetState)> = match &invite.ratchet {
        Some(prekey) => {
            let alice_x: [u8; 32] = B64
                .decode(&invite.keys.x25519_pub_b64)
                .map_err(|e| format!("invite x25519 b64: {e}"))?
                .try_into()
                .map_err(|_| "invite x25519 wrong size".to_string())?;
            let alice_kem = B64
                .decode(&invite.keys.ml_kem_pub_b64)
                .map_err(|e| format!("invite ml-kem b64: {e}"))?;
            let prekey_pub: [u8; 32] = B64
                .decode(&prekey.dh_pub_b64)
                .map_err(|e| format!("invite ratchet prekey b64: {e}"))?
                .try_into()
                .map_err(|_| "invite ratchet prekey wrong size".to_string())?;
            let (eph_priv, eph_pub) = crypto::ratchet_keypair();
            let x3dh = crypto::dh(&eph_priv, &alice_x);
            let (kem_ct, kem_ss) = crypto::ml_kem_encapsulate_local(&alice_kem)?;
            let sk = crypto::root_init(&x3dh, &kem_ss);
            let state = ratchet_init_initiator(sk, prekey_pub);
            let bootstrap = RatchetBootstrap {
                eph_pub_b64: B64.encode(eph_pub),
                kem_ct_b64: B64.encode(&kem_ct),
                dh_pub_b64: B64.encode(b32(&state.dhs_pub)?),
                // Advertise our initial rolling KEM pub so the inviter's first
                // sending chain encapsulates to it (hybrid from message one of
                // the inviter→accepter direction).
                kem_pub_b64: state.kem_pub.clone(),
            };
            Some((bootstrap, state))
        }
        None => None,
    };

    let contact = DmContact {
        did: invite.did.clone(),
        peer_ticket: invite.node_ticket.clone(),
        // F-OWNER-TICKET-PoP: this is the INVITED member's OWN node ticket, taken
        // from the invite they themselves authored ⇒ self-asserted.
        ticket_self_asserted: true,
        name: invite.name.clone(),
        last_ts: now_ms(),
        last_preview: String::from("Invite accepted"),
        unread: 0,
        my_inbound_queue: Some(my_queue.clone()),
        my_recv_pseudonym: Some(my_recv_pseudonym),
        their_inbound_queue: Some(invite.queue.clone()),
        my_send_pseudonym: Some(my_send_pseudonym.clone()),
        peer_pubkeys: Some(invite.keys.clone()),
        key_pop: None,
        status: ContactStatus::Active,
        mode,
        anon_identity: anon,
        ratchet_capable: ratchet_bootstrap.is_some(),
        key_verified: true,
        key_changed: false,
        oob_verified: false,
        my_queue_rotated_at: 0,
        my_queue_msg_count: 0,
        retired_queues: Vec::new(),
        salted_queue: None,
        peer_salted: false,
        peer_salted_at: 0,
        salted_self_ready_at: 0,
        needs_verify_before_send: false,
    };
    let _ = upsert_contact_record(contact)
        .await
        .map_err(|e| e.to_string())?;

    // Persist the bootstrapped ratchet state under the peer's real DID.
    if let Some((_, state)) = &ratchet_bootstrap {
        write_ratchet(&invite.did, state)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Join our new inbound queue so the receiver picks up their replies.
    let _ = peer::join_topic(&format!("{TOPIC_PREFIX_V2}/{my_queue}")).await;

    // Build & send the handshake reply on THEIR queue. When we bootstrapped a
    // ratchet, the block tells the inviter how to recover SK + our first DH key.
    let mut handshake_body = json!({
        "my_inbound_queue": my_queue,
        "name": share_name,
        "pubkeys": share_pub,
    });
    if let Some((bootstrap, _)) = &ratchet_bootstrap {
        handshake_body["ratchet"] =
            serde_json::to_value(bootstrap).map_err(|e| format!("ratchet block: {e}"))?;
    }
    // Tell the inviter our node ticket so its runtime can bootstrap the mesh to
    // OUR queue (mirror of the invite carrying the inviter's ticket). F-FOLLOW
    // ANNOUNCE-TICKET-LEAK (sibling): IP-cap this too — the handshake reply is a
    // KIND_HANDSHAKE so it bypasses the KIND_MESSAGE `nt` compaction above; without
    // this it would hand the inviter our full direct-IP set on first contact.
    if let Some(t) = peer::my_ticket().await {
        handshake_body["node_ticket"] = serde_json::Value::String(compact_nt_ticket(&t));
    }

    let inner = build_inner(KIND_HANDSHAKE, &handshake_body, &my_did, &my_seed_hex, None).await?;
    let envelope = encrypt_inner_for_peer(&inner, &invite.keys)?;
    let wire = json!({
        "type": "dm.v2",
        "envelope": envelope,
    })
    .to_string();

    let topic = format!("{TOPIC_PREFIX_V2}/{}", invite.queue);
    // Bootstrap the gossip mesh to the inviter's runtime via its node ticket so
    // the handshake actually reaches them across runtimes (not just same-host).
    let boot: Vec<String> = invite.node_ticket.iter().cloned().collect();
    let _ = peer::join_topic_with(&topic, &boot).await;
    // Sealed-sender at the provider layer: random pseudonym, not DID.
    // outbox::publish_or_enqueue uses a constant "v2-sealed" placeholder
    // for the outer signature (providers that validate non-empty don't
    // reject; the real sig is inside the envelope). It queues for retry
    // unless delivery is CONFIRMED (sent + a topic neighbor exists); `boot`
    // lets the retry re-graft the mesh to the inviter's runtime.
    let _ = crate::api::outbox::publish_or_enqueue(&topic, &boot, &my_send_pseudonym, &wire).await;

    Ok(invite.did)
}

// ── Sealed-sender envelope plumbing ──────────────────────────────────
//
// Inner payload — what lives inside the ChaCha20-Poly1305 ciphertext.
// The provider never sees this.

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InnerPayload {
    kind: String,
    sender_did: String,
    ts: i64,
    body: Value,
    /// Ed25519 sig over `canonicalize({kind, body, sender_did, ts[, rh]})`.
    /// `rh` is in the signed set ONLY when present (must-fix #1) — including it
    /// unconditionally would emit `"rh":null` and break the signature on every
    /// pre-ratchet message + every not-yet-upgraded peer.
    sig: String,
    /// Double Ratchet header (sealed + signed). Present ⇒ ratchet message;
    /// absent ⇒ single-shot (legacy) path. Echoed UNSEALED in the wire `rh`
    /// so the receiver can pick the key + bound skips before decrypting; the
    /// two MUST match (checked post-decrypt) or the message is rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rh: Option<RatchetHeader>,
}

/// Shared identity namespace for the runtime provider — one did:key per user
/// across every Hey capsule. (Re-exported from runtime so all signing sites
/// use the same value.)
const IDENTITY_NS: &str = crate::runtime::identity_provider::HEY_NAMESPACE;

/// Sign `payload`: with the local Ed25519 seed when `auth_key_hex` is set
/// (local session or a per-contact anonymous identity), or via the runtime
/// identity provider when it is EMPTY (provider-backed session — the key is
/// runtime-held, no passkey tap, the wallet model). One branch point keeps
/// every signing site mode-agnostic.
async fn sign_bytes(payload: &[u8], auth_key_hex: &str) -> Result<String, String> {
    if auth_key_hex.is_empty() {
        let resp = crate::runtime::identity_provider::sign(IDENTITY_NS, payload)
            .await
            .map_err(|e| format!("provider sign: {e}"))?;
        let d = resp.get("data").unwrap_or(&resp);
        d.get("signature_hex")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "provider sign: no signature_hex".to_string())
    } else {
        let seed = seed32(auth_key_hex)?;
        Ok(sign(payload, &seed))
    }
}

/// Bytes signed for an inner payload. `rh` is added to the canonicalized
/// object ONLY when present (must-fix #1), so a single-shot message signs
/// EXACTLY the bytes it did before the ratchet existed.
fn inner_sign_bytes(
    kind: &str,
    body: &Value,
    sender_did: &str,
    ts: i64,
    rh: Option<&RatchetHeader>,
) -> String {
    inner_sign_bytes_bound(kind, body, sender_did, ts, rh, None, None)
}

/// Canonical signed bytes for an inner payload. F-08: when `recipient` and
/// `conv` are BOTH supplied, they are folded into the signed object so the
/// signature is bound to a specific recipient + conversation (queue/group) and
/// a mutual contact can't re-seal a message into a DIFFERENT conversation. When
/// either is `None` the extra keys are OMITTED, producing the byte-for-byte
/// LEGACY canonical form — so a peer that hasn't upgraded still verifies (the
/// verifier tries the new form, then falls back to this legacy form).
fn inner_sign_bytes_bound(
    kind: &str,
    body: &Value,
    sender_did: &str,
    ts: i64,
    rh: Option<&RatchetHeader>,
    recipient: Option<&str>,
    conv: Option<&str>,
) -> String {
    let mut obj = json!({
        "kind": kind,
        "body": body,
        "sender_did": sender_did,
        "ts": ts,
    });
    if let Some(h) = rh {
        obj["rh"] = serde_json::to_value(h).unwrap_or(Value::Null);
    }
    if let (Some(r), Some(cv)) = (recipient, conv) {
        obj["recipient"] = Value::String(r.to_string());
        obj["conv"] = Value::String(cv.to_string());
    }
    canonicalize(&obj)
}

/// Self-updating friendsbook: a peer stamps its CURRENT node ticket on every
/// signed message; we keep their latest relay/address so we can re-find them
/// after THEY change networks/relays or update/reboot. Writes only on a change.
/// `self_asserted` ⇒ the ticket was carried in a message whose `sender_did` we
/// VERIFIED equals `did` (the contact asserting their OWN endpoint). Only then is
/// the ticket trustworthy as a group-call dial anchor. A group OWNER bootstrapping
/// a member's ticket from its roster passes false ⇒ the ticket is recorded for
/// discovery/pair-queue meshing but does NOT become a trusted dial endpoint.
async fn refresh_peer_ticket(did: &str, nt: &str, self_asserted: bool) {
    // A node ticket is a base32 EndpointAddr — comfortably under 1 KB. Reject
    // empty/oversized values so a peer can't bloat contacts.json (cheap DoS).
    if nt.is_empty() || nt.len() > 1024 {
        return;
    }
    let _g = contacts_gate().lock().await; // serialize contact RMW (race fix)
    let mut list = list_contacts().await;
    let mut changed = false;
    for c in list.iter_mut() {
        if c.did != did {
            continue;
        }
        // F-ROSTER-TICKET-REPOINT: a non-self-asserted (owner-roster) refresh must NEVER
        // overwrite a ticket the member SELF-asserted. The old code overwrote the VALUE on
        // any difference while only ever raising the flag, so an owner refresh left
        // peer_ticket=Eve with ticket_self_asserted=true — poisoning BOTH the group dial
        // (peer_ticket_self_asserted) and the 1:1 dial (raw peer_ticket) to a non-member.
        if !self_asserted && c.ticket_self_asserted {
            break; // keep the member's own self-asserted ticket; did is unique
        }
        if c.peer_ticket.as_deref() != Some(nt) || (self_asserted && !c.ticket_self_asserted) {
            c.peer_ticket = Some(nt.to_string());
            // Couple flag to provenance: a self-assertion upgrades to trusted; a discovery
            // (owner) refresh of an untrusted slot stays untrusted (false) — value + flag
            // move together so they can never decouple again.
            c.ticket_self_asserted = self_asserted;
            changed = true;
        }
        break; // did is unique — done after the matching contact
    }
    if changed {
        let _ = write_contacts(&list).await;
    }
}

/// F-OWNER-TICKET-PoP: a contact's peer node ticket, but ONLY when that ticket was
/// SELF-ASSERTED by the contact (a message we verified came from them). Returns
/// None for an owner-roster-bootstrapped or owner-poisoned ticket, so the
/// group-call dial anchor can never be redirected to a non-member (Eve) by a
/// malicious owner. Mirrors the plain `peer_ticket` lookup (list_contacts/find).
pub async fn peer_ticket_self_asserted(did: &str) -> Option<String> {
    list_contacts()
        .await
        .into_iter()
        .find(|c| c.did == did)
        .filter(|c| c.ticket_self_asserted)
        .and_then(|c| c.peer_ticket)
}

/// F-FOLLOWANNOUNCE-TICKET-LEAK: slim our carrier node ticket before it is stamped
/// into the SIGNED `nt` field of every outgoing DM (the self-updating friendsbook).
/// Keeps ALL relays + up to `MAX_NT_IP_ADDRS` direct IPs (same-LAN dial hints, the
/// same treatment the shareable friend link gets in hey-mobile-runtime's
/// `compact_ticket`) and DROPS the rest, so a contact never receives our FULL
/// direct-IP set on every message. Round-trips the carrier's base32 (legacy base64
/// tolerated); falls back to the input unchanged on any decode/encode error so a
/// re-find never breaks. Mirrors the canonical compaction so both emitters agree.
fn compact_nt_ticket(ticket: &str) -> String {
    // Cap matches hey-mobile-runtime::social::compact_ticket — relays kept in full;
    // a small bounded set of direct IPs retained for relay-less same-LAN meshing.
    const MAX_NT_IP_ADDRS: usize = 4;
    let bytes = data_encoding::BASE32_NOPAD
        .decode(ticket.as_bytes())
        .ok()
        .or_else(|| B64URL.decode(ticket).ok());
    let Some(bytes) = bytes else { return ticket.to_string() };
    let Ok(mut v) = serde_json::from_slice::<Value>(&bytes) else { return ticket.to_string() };
    if let Some(addrs) = v.get_mut("addrs").and_then(|a| a.as_array_mut()) {
        // TransportAddr is externally tagged: {"Relay":..} | {"Ip":..}.
        let mut ip_kept = 0usize;
        addrs.retain(|e| {
            if e.get("Relay").is_some() {
                true
            } else if e.get("Ip").is_some() && ip_kept < MAX_NT_IP_ADDRS {
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

async fn build_inner(
    kind: &str,
    body: &Value,
    sender_did: &str,
    auth_key_hex: &str,
    rh: Option<RatchetHeader>,
) -> Result<InnerPayload, String> {
    build_inner_bound(kind, body, sender_did, auth_key_hex, rh, None, None).await
}

/// Like `build_inner` but binds the signature to a recipient + conversation tag
/// (F-08). Pass `recipient`/`conv` for message sends on a known per-pair queue;
/// pass `None` (via `build_inner`) for handshake/welcome/self-test where there
/// is no settled conversation yet (those keep the legacy form).
#[allow(clippy::too_many_arguments)]
async fn build_inner_bound(
    kind: &str,
    body: &Value,
    sender_did: &str,
    auth_key_hex: &str,
    rh: Option<RatchetHeader>,
    recipient: Option<&str>,
    conv: Option<&str>,
) -> Result<InnerPayload, String> {
    let ts = now_ms();
    // Self-updating friendsbook: stamp our CURRENT node ticket onto every real
    // message body (which is SIGNED, so a peer can trust it and an attacker can't
    // forge it). The receiver refreshes our stored ticket, so we stay re-findable
    // after WE change networks/relays — even across the peer's app update/reboot.
    let body: Value = if kind == KIND_MESSAGE {
        let mut b = body.clone();
        if let Some(t) = peer::my_ticket().await {
            // F-FOLLOWANNOUNCE-TICKET-LEAK: IP-cap the ticket before stamping it.
            // The follow-announce (send_follow_announce -> chat_send -> send_message
            // -> KIND_MESSAGE) and every regular DM ride this `nt` field; stamping
            // the FULL `my_ticket()` here defeated the caller's compact_ticket(). Now
            // `nt` carries relays + a few same-LAN hints only — never our full
            // direct-IP set. Re-find still works (relay -> iroh upgrades to direct).
            b["nt"] = Value::String(compact_nt_ticket(&t));
        }
        b
    } else {
        body.clone()
    };
    let to_sign = inner_sign_bytes_bound(kind, &body, sender_did, ts, rh.as_ref(), recipient, conv);
    let sig = sign_bytes(to_sign.as_bytes(), auth_key_hex).await?;
    Ok(InnerPayload {
        kind: kind.into(),
        sender_did: sender_did.into(),
        ts,
        body,
        sig,
        rh,
    })
}

fn verify_inner(inner: &InnerPayload) -> bool {
    verify_inner_bound(inner, None, None)
}

/// Verify an inner signature, F-08 backward-compatibly. If `recipient`/`conv`
/// are supplied we FIRST check the NEW recipient+conversation-bound canonical
/// form; if that fails (or the binding wasn't supplied) we fall back to the
/// LEGACY form (no recipient/conv keys). This means a message from a peer that
/// already binds verifies via the new path, while a message from a not-yet-
/// upgraded peer still verifies via the legacy path — delivery is never broken.
fn verify_inner_bound(
    inner: &InnerPayload,
    recipient: Option<&str>,
    conv: Option<&str>,
) -> bool {
    if !inner.sender_did.starts_with("did:key:z") {
        return false;
    }
    let pk = match did_key_to_public_key(&inner.sender_did) {
        Ok(p) => p,
        Err(_) => return false,
    };
    // New (with-recipient) form first — only when a binding was provided.
    if recipient.is_some() && conv.is_some() {
        let bound = inner_sign_bytes_bound(
            &inner.kind,
            &inner.body,
            &inner.sender_did,
            inner.ts,
            inner.rh.as_ref(),
            recipient,
            conv,
        );
        if verify(bound.as_bytes(), &inner.sig, &pk) {
            return true;
        }
    }
    // Legacy (no-recipient) form — back-compat with not-yet-upgraded peers.
    let legacy = inner_sign_bytes(
        &inner.kind,
        &inner.body,
        &inner.sender_did,
        inner.ts,
        inner.rh.as_ref(),
    );
    verify(legacy.as_bytes(), &inner.sig, &pk)
}

fn encrypt_inner_for_peer(
    inner: &InnerPayload,
    peer_keys: &PeerKeys,
) -> Result<HpqEnvelope, String> {
    let plaintext = serde_json::to_string(inner).map_err(|e| format!("inner json: {e}"))?;
    let recipient_x25519: [u8; 32] = B64
        .decode(&peer_keys.x25519_pub_b64)
        .map_err(|e| format!("peer x25519 b64: {e}"))?
        .try_into()
        .map_err(|_| "peer x25519 wrong size".to_string())?;
    let recipient_kem = B64
        .decode(&peer_keys.ml_kem_pub_b64)
        .map_err(|e| format!("peer ml-kem b64: {e}"))?;
    crypto::encrypt_to_hybrid(&plaintext, &recipient_x25519, &recipient_kem)
}

async fn decrypt_envelope_to_inner(
    env: &HpqEnvelope,
    via: &DecryptVia,
) -> Result<InnerPayload, String> {
    let pt = open_envelope(env, via).await?;
    serde_json::from_str(&pt).map_err(|e| format!("inner deserialize: {e}"))
}

/// ONE-WAY PAIRING key-share. Seal a small JSON bundle (the sender's PQ pubkeys +
/// node ticket + name) to a peer's hybrid pubkeys, so it can ride the PUBLIC
/// follow event on THAT peer's feed topic — a channel the peer already subscribes
/// — yet stay opaque to every other feed subscriber (no social-graph / DM-key
/// leak). This is the ONLY channel that reaches a brand-new followee one-way: the
/// metadata-safe per-pair queue can't carry it, because the followee doesn't yet
/// know our DID and so can't subscribe its inbound pair queue (the receive path
/// is gated on an existing Active contact). Output is the `HpqEnvelope` as a JSON
/// string. Encoding mirrors `encrypt_inner_for_peer`.
pub fn seal_bundle_for_peer(peer: &PeerKeys, plaintext: &str) -> Result<String, String> {
    let recipient_x25519: [u8; 32] = B64
        .decode(&peer.x25519_pub_b64)
        .map_err(|e| format!("peer x25519 b64: {e}"))?
        .try_into()
        .map_err(|_| "peer x25519 wrong size".to_string())?;
    let recipient_kem = B64
        .decode(&peer.ml_kem_pub_b64)
        .map_err(|e| format!("peer ml-kem b64: {e}"))?;
    let env = crypto::encrypt_to_hybrid(plaintext, &recipient_x25519, &recipient_kem)?;
    serde_json::to_string(&env).map_err(|e| format!("seal bundle json: {e}"))
}

/// Open a one-way-pairing key-share that was sealed to US, via the SESSION decrypt
/// path (provider OR local seed — the same path inbound DMs use, so it works on
/// both a wallet/provider-backed identity and a local-seed one). Input is the
/// `HpqEnvelope` JSON string from a follow event's `enc` field; returns the bundle
/// plaintext. Decrypting it is proof the sender held OUR pubkeys (from our QR /
/// friend-link) — i.e. an invited pairing, safe to materialize a DM contact for.
pub async fn open_bundle_for_me(env_str: &str) -> Result<String, String> {
    let env: HpqEnvelope =
        serde_json::from_str(env_str).map_err(|e| format!("bundle env parse: {e}"))?;
    let via = decrypt_via_for_session()?;
    open_envelope(&env, &via).await
}

// ── Double Ratchet state machine (M6 stage 2) ────────────────────────
//
// Signal-style Double Ratchet layered onto the v2 sealed-sender wire. It
// changes ONLY what the X25519-half feeding `crypto::derive_key` IS: a
// forward-secret chain message key `mk` instead of a per-message static ECDH.
// The frozen hpq envelope / ChaCha / padding / HKDF_INFO are untouched.
//
//   * The sender's CURRENT ratchet DH pubkey rides UNSEALED in `envelope.eph`.
//   * The cleartext wire `rh = {pn, n}` lets the receiver pick the right key
//     and bound skips BEFORE decrypting (the page number — must-fix #7); a
//     forged `rh` either fails the AEAD (wrong key) or the post-decrypt check
//     against the SEALED+SIGNED `InnerPayload.rh` (which carries dh,pn,n).
//   * Every ongoing ratchet DH key is a LOCAL ephemeral (the provider can't
//     hold a rotating key). The provider/anon key is used only at bootstrap
//     and for the per-message ML-KEM half (`ratchet_kem_ss`). The ROLLING
//     KEM-ratchet private (`kem_priv`) is likewise a local ephemeral, held in
//     the ratchet file even for provider-backed sessions — its ct is
//     decapsulated LOCALLY, never via the provider (same as `dhs_priv`).
//
// SCOPE: classical X25519 always delivers FS + PCS. When BOTH sides bootstrap
// after the hybrid upgrade, a rolling ML-KEM secret is ALSO folded into the
// root KDF on every turn (`kdf_rk_hybrid`) — so PCS becomes POST-QUANTUM:
// recovery after a turn the attacker didn't observe needs breaking BOTH X25519
// and ML-KEM-768. The per-message ML-KEM seal to a STATIC key is retained
// (harvest-now confidentiality + RK0 floor). Pre-upgrade contacts, and the
// accepter's first chain before the first turn, remain classical-only — there
// is no PQ-PCS until the first hybrid DH round-trip the attacker didn't observe.

/// The unsealed page-number header. `dh` is base64 and equals `envelope.eph`;
/// it is duplicated here so the SIGNED copy commits the sender to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatchetHeader {
    /// Base64 — the sender's current ratchet DH public key (== envelope.eph).
    pub dh: String,
    /// Length of the sender's PREVIOUS sending chain (for old-chain skips).
    pub pn: u32,
    /// Index of this message in the sender's current sending chain.
    pub n: u32,
    /// Base64 — ML-KEM-768 ciphertext encapsulated to the RECEIVER's current
    /// rolling KEM public key (the hybrid KEM-ratchet half). Present only on a
    /// hybrid sending chain; the receiver folds the decapsulated secret into
    /// `kdf_rk_hybrid` on the turn this header starts. Carried on EVERY message
    /// of the chain (constant), so losing the chain's first message doesn't
    /// strand the turn. Absent ⇒ classical chain (legacy/warm-up).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kem_ct: Option<String>,
    /// Base64 — the sender's CURRENT rolling ML-KEM public key, so the receiver
    /// can encapsulate to it on its next turn. Advertised whenever we hold a
    /// rolling KEM keypair (i.e. a ratchet bootstrapped after the hybrid
    /// upgrade); None for pre-upgrade contacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kem_pub: Option<String>,
}

/// One out-of-order message key we derived early and stashed. Keyed by the
/// chain pubkey (hex) it belongs to + its index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedKey {
    /// Hex — the peer ratchet pubkey (DHr) of the chain this key belongs to.
    pub dh: String,
    pub n: u32,
    /// Hex — the 32-byte message key.
    pub mk: String,
    /// When stored (ms) — for TTL eviction.
    pub stored_at: i64,
}

/// Per-contact Double Ratchet state. All key material is hex. Persisted in
/// dm/ratchet/<did>.json. A "prekey-only" state (empty `rk`) is written by the
/// inviter at `generate_invite` and completed at `receive_handshake`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetState {
    /// Root key (hex 32B). Empty ⇒ a not-yet-bootstrapped prekey stash.
    pub rk: String,
    /// Sending chain key (hex). None until a sending chain exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cks: Option<String>,
    /// Receiving chain key (hex). None until we've received from a peer chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckr: Option<String>,
    /// Our current ratchet DH keypair (hex 32B each).
    pub dhs_priv: String,
    pub dhs_pub: String,
    /// Their current ratchet DH public key (hex). None until first received.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dhr_pub: Option<String>,
    #[serde(default)]
    pub ns: u32,
    #[serde(default)]
    pub nr: u32,
    #[serde(default)]
    pub pn: u32,
    #[serde(default)]
    pub skipped: Vec<SkippedKey>,

    // ── Hybrid PQ KEM-ratchet (rolling ML-KEM, folded into kdf_rk on turns).
    //    All None ⇒ a classical (pre-upgrade) ratchet. A bootstrap done after
    //    the hybrid upgrade mints these; they ride alongside the X25519 ratchet.
    /// Base64 — OUR current rolling ML-KEM decapsulation (private) key. Rotated
    /// (old one discarded → PQ-PCS) on every sending turn, exactly like dhs_priv.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kem_priv: Option<String>,
    /// Base64 — OUR current rolling ML-KEM public key (the one we advertise so
    /// the peer encapsulates to it). Matches `kem_priv`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kem_pub: Option<String>,
    /// Base64 — the PEER's last-advertised rolling ML-KEM public key; what we
    /// encapsulate to when we mint a new sending chain. None ⇒ peer hasn't
    /// advertised one yet (classical until it does).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_kem_pub: Option<String>,
    /// Base64 — the KEM ciphertext for our CURRENT sending chain (encapsulated
    /// to `peer_kem_pub` when the chain was minted). Attached to every message
    /// of the chain so any one of them lets the peer turn. None ⇒ classical
    /// sending chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_kem_ct: Option<String>,
}

/// Defense-in-depth: wipe the long-lived secret key material from the heap when a
/// RatchetState drops (every clone-then-persist temporary, and on logout). The
/// at-rest copy is already DEK-encrypted; this clears the IN-MEMORY copy so it
/// can't linger in freed pages. Public keys / ciphertexts are left as-is.
impl Drop for RatchetState {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.rk.zeroize();
        self.dhs_priv.zeroize();
        if let Some(s) = self.cks.as_mut() {
            s.zeroize();
        }
        if let Some(s) = self.ckr.as_mut() {
            s.zeroize();
        }
        if let Some(s) = self.kem_priv.as_mut() {
            s.zeroize();
        }
        for sk in &mut self.skipped {
            sk.mk.zeroize();
        }
    }
}

impl RatchetState {
    /// True once SK is established (not just a prekey stash).
    fn is_bootstrapped(&self) -> bool {
        !self.rk.is_empty()
    }
}

/// Ratchet prekey advertised in an invite (the inviter's initial DH public
/// key). Additive + optional, so old invites (no field) simply don't ratchet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetPrekey {
    pub dh_pub_b64: String,
}

/// Ratchet bootstrap block carried in a handshake by the accepter: a discarded
/// bootstrap ephemeral + ML-KEM ct (→ SK) and the accepter's first ratchet DH
/// pubkey (so the inviter can establish both chains immediately).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RatchetBootstrap {
    eph_pub_b64: String,
    kem_ct_b64: String,
    dh_pub_b64: String,
    /// The accepter's INITIAL rolling ML-KEM public key (hybrid KEM-ratchet).
    /// Additive + optional: a handshake WITHOUT it (old accepter) keeps the
    /// responder on the classical ratchet. Present ⇒ the inviter's first
    /// sending chain encapsulates to it and goes hybrid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kem_pub_b64: Option<String>,
}

// ── ratchet hex helpers ──
fn hx(b: &[u8]) -> String {
    bytes_to_hex(b)
}
fn b32(hex: &str) -> Result<[u8; 32], String> {
    seed32(hex)
}

// ── per-contact ratchet store (must-fix #7: own file, never on contacts.json) ──

fn ratchet_path(did: &str) -> String {
    let safe = did.replace(['/', ':'], "_");
    format!("{RATCHET_DIR}/{safe}.json")
}

async fn read_ratchet(did: &str) -> Option<RatchetState> {
    storage::read_json(&ratchet_path(did))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
}

/// Persist ratchet state. The per-app storage path (/api/apps/<id>/storage/)
/// is OVERWRITE-capable — proven by the message-append + mark_read flows that
/// rewrite dm/*.json on every message — so the advance is durable (must-fix
/// #7's "confirm contacts.json is overwrite-capable, not create-only").
async fn write_ratchet(did: &str, st: &RatchetState) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(st).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(&ratchet_path(did), &v).await
}

async fn remove_ratchet(did: &str) {
    let _ = storage::remove(&ratchet_path(did)).await;
}

// ── pure ratchet core (no I/O — unit-testable by self_test_ratchet) ──

/// Initiator init (the invite ACCEPTER). We hold SK and the inviter's ratchet
/// prekey pub; establish a sending chain at once. Maps to Signal RatchetInitAlice.
fn ratchet_init_initiator(sk: [u8; 32], peer_prekey_pub: [u8; 32]) -> RatchetState {
    let (dhs_priv, dhs_pub) = crypto::ratchet_keypair();
    // The first sending chain is CLASSICAL: we don't know the inviter's rolling
    // KEM pub yet, so we can't encapsulate to it. We still mint our own rolling
    // KEM keypair to advertise (in the handshake + on messages) so the inviter
    // can encapsulate to us — the conversation goes hybrid on the first turn.
    let (rk, cks) = crypto::kdf_rk(&sk, &crypto::dh(&dhs_priv, &peer_prekey_pub));
    let (kem_priv, kem_pub) = crypto::generate_ml_kem_keypair();
    RatchetState {
        rk: hx(&rk),
        cks: Some(hx(&cks)),
        ckr: None,
        dhs_priv: hx(&dhs_priv),
        dhs_pub: hx(&dhs_pub),
        dhr_pub: Some(hx(&peer_prekey_pub)),
        ns: 0,
        nr: 0,
        pn: 0,
        skipped: Vec::new(),
        kem_priv: Some(B64.encode(&kem_priv)),
        kem_pub: Some(B64.encode(&kem_pub)),
        peer_kem_pub: None,
        send_kem_ct: None,
    }
}

/// Responder init (the INVITER). DHs = our published prekey; RK = SK. We then
/// turn the ratchet immediately against the accepter's first ratchet key so we
/// can BOTH send and receive right away (Signal would defer this to first
/// receive — equivalent, since the accepter's first message carries this dh).
/// Maps to Signal RatchetInitBob + one DHRatchet.
fn ratchet_init_responder(
    sk: [u8; 32],
    prekey_priv: [u8; 32],
    prekey_pub: [u8; 32],
    peer_dh_pub: [u8; 32],
    peer_kem_pub: Option<Vec<u8>>,
) -> Result<RatchetState, String> {
    // Mint our rolling KEM keypair. If the accepter advertised its rolling KEM
    // pub in the handshake, our first sending chain (minted in the dh_ratchet
    // below) encapsulates to it and goes hybrid. The RECEIVING chain for the
    // accepter's FIRST chain is always classical (it was sent before any rolling
    // KEM existed), so we pass recv_kem_ss = None here.
    let (kem_priv, kem_pub) = crypto::generate_ml_kem_keypair();
    let mut st = RatchetState {
        rk: hx(&sk),
        cks: None,
        ckr: None,
        dhs_priv: hx(&prekey_priv),
        dhs_pub: hx(&prekey_pub),
        dhr_pub: None,
        ns: 0,
        nr: 0,
        pn: 0,
        skipped: Vec::new(),
        kem_priv: Some(B64.encode(&kem_priv)),
        kem_pub: Some(B64.encode(&kem_pub)),
        peer_kem_pub: peer_kem_pub.map(|p| B64.encode(p)),
        send_kem_ct: None,
    };
    dh_ratchet(&mut st, peer_dh_pub, None)?;
    Ok(st)
}

/// Turn the DH ratchet on a freshly-seen peer key: finish nothing here (the
/// caller skips the old chain first), derive the new receiving chain, then mint
/// a FRESH sending keypair (the old `dhs_priv` is overwritten and never reused
/// — that discard is what delivers PCS; must-fix #5).
///
/// Hybrid KEM-ratchet (when both sides bootstrapped post-upgrade):
///   * RECV chain — if the triggering message carried a rolling KEM ciphertext,
///     the caller decapsulates it (with our current `kem_priv`) and passes the
///     secret as `recv_kem_ss`; it's folded into the receiving root step.
///   * SEND chain — if we know the peer's rolling KEM pub (`peer_kem_pub`), we
///     encapsulate to it, fold the secret into the sending root step, rotate
///     OUR rolling KEM keypair (discard the old private → PQ-PCS), and stash the
///     ciphertext (`send_kem_ct`) for ratchet_step_send to attach to messages.
fn dh_ratchet(
    st: &mut RatchetState,
    dh_pub: [u8; 32],
    recv_kem_ss: Option<&[u8]>,
) -> Result<(), String> {
    let dhs_priv = b32(&st.dhs_priv)?;
    let rk0 = b32(&st.rk)?;
    st.pn = st.ns;
    st.ns = 0;
    st.nr = 0;
    // Receiving chain (hybrid iff the incoming turn carried a KEM secret).
    let dh_recv = crypto::dh(&dhs_priv, &dh_pub);
    let (rk1, ckr) = match recv_kem_ss {
        Some(ss) => crypto::kdf_rk_hybrid(&rk0, &dh_recv, ss),
        None => crypto::kdf_rk(&rk0, &dh_recv),
    };
    st.dhr_pub = Some(hx(&dh_pub));
    st.ckr = Some(hx(&ckr));
    // Sending chain (hybrid iff we hold the peer's rolling KEM pub).
    let (new_priv, new_pub) = crypto::ratchet_keypair();
    let dh_send = crypto::dh(&new_priv, &dh_pub);
    let (rk2, cks) = if let Some(peer_kem_b64) = st.peer_kem_pub.clone() {
        let peer_kem = B64
            .decode(&peer_kem_b64)
            .map_err(|e| format!("peer_kem_pub b64: {e}"))?;
        let (kem_ct, kem_ss) = crypto::ml_kem_encapsulate_local(&peer_kem)?;
        let (rk2, cks) = crypto::kdf_rk_hybrid(&rk1, &dh_send, &kem_ss);
        // Rotate our rolling KEM keypair; overwriting `kem_priv` discards the
        // old private (the carrier String is freed) — that discard is what makes
        // PCS post-quantum, mirroring the dhs_priv discard above.
        let (new_kem_priv, new_kem_pub) = crypto::generate_ml_kem_keypair();
        st.kem_priv = Some(B64.encode(&new_kem_priv));
        st.kem_pub = Some(B64.encode(&new_kem_pub));
        st.send_kem_ct = Some(B64.encode(&kem_ct));
        (rk2, cks)
    } else {
        st.send_kem_ct = None;
        crypto::kdf_rk(&rk1, &dh_send)
    };
    st.dhs_priv = hx(&new_priv);
    st.dhs_pub = hx(&new_pub);
    st.rk = hx(&rk2);
    st.cks = Some(hx(&cks));
    Ok(())
}

/// Advance the receiving chain up to (but not including) `until`, stashing each
/// skipped message key. Rejects an implausible jump BEFORE any KDF (must-fix
/// #7 — the cleartext `n`/`pn` make this a pre-KDF check, not a bounded loop).
fn skip_message_keys(st: &mut RatchetState, until: u32) -> Result<(), String> {
    if until > st.nr.saturating_add(MAX_SKIP) {
        return Err(format!(
            "ratchet: would skip past MAX_SKIP ({} > {} + {MAX_SKIP})",
            until, st.nr
        ));
    }
    let Some(ckr_hex) = st.ckr.clone() else {
        return Ok(()); // no receiving chain yet — nothing to skip
    };
    let dhr = st
        .dhr_pub
        .clone()
        .ok_or_else(|| "skip without dhr".to_string())?;
    let mut ckr = b32(&ckr_hex)?;
    let now = now_ms();
    while st.nr < until {
        let (mk, ckr_next) = crypto::kdf_ck(&ckr);
        st.skipped.push(SkippedKey {
            dh: dhr.clone(),
            n: st.nr,
            mk: hx(&mk),
            stored_at: now,
        });
        ckr = ckr_next;
        st.nr += 1;
    }
    st.ckr = Some(hx(&ckr));
    evict_skipped(st);
    Ok(())
}

/// TTL + FIFO eviction of stored skipped keys (bounds memory; must-fix #7).
fn evict_skipped(st: &mut RatchetState) {
    let cutoff = now_ms() - SKIPPED_TTL_MS;
    st.skipped.retain(|k| k.stored_at >= cutoff);
    if st.skipped.len() > MAX_SKIPPED_KEYS {
        let drop = st.skipped.len() - MAX_SKIPPED_KEYS;
        st.skipped.drain(0..drop); // oldest first
    }
}

/// Consume a previously-stashed out-of-order key for (`dh_hex`, `n`), if any.
fn try_skipped(st: &mut RatchetState, dh_hex: &str, n: u32) -> Result<Option<[u8; 32]>, String> {
    if let Some(pos) = st.skipped.iter().position(|k| k.dh == dh_hex && k.n == n) {
        let k = st.skipped.remove(pos);
        Ok(Some(b32(&k.mk)?))
    } else {
        Ok(None)
    }
}

/// Advance the SENDING chain one step → (message key, header to put on the
/// wire). Caller MUST persist the advanced state BEFORE using `mk` for anything
/// durable, so a crash can never reuse `ns`/`mk` (must-fix #5: no mk reuse).
fn ratchet_step_send(st: &mut RatchetState) -> Result<([u8; 32], RatchetHeader), String> {
    let cks = st
        .cks
        .clone()
        .ok_or_else(|| "ratchet has no sending chain yet".to_string())?;
    let (mk, cks_next) = crypto::kdf_ck(&b32(&cks)?);
    st.cks = Some(hx(&cks_next));
    let header = RatchetHeader {
        dh: B64.encode(b32(&st.dhs_pub)?),
        pn: st.pn,
        n: st.ns,
        // Hybrid: advertise our rolling KEM pub (so the peer encapsulates to us)
        // and carry this chain's KEM ct (so the peer can turn). Both constant
        // across the chain; None on a classical chain. (must-fix: carried on
        // every message for robustness against a lost first-of-epoch.)
        kem_ct: st.send_kem_ct.clone(),
        kem_pub: st.kem_pub.clone(),
    };
    st.ns += 1;
    Ok((mk, header))
}

/// Advance the RECEIVING ratchet to position (`dh`, `n`) and return its message
/// key. Operates on a CLONE supplied by the caller: on any failure (bad jump,
/// AEAD mismatch downstream, old/garbage epoch) the caller discards the clone,
/// so a forged/replayed message can never corrupt the committed state.
fn ratchet_step_recv(
    st: &mut RatchetState,
    dh_hex: &str,
    dh_bytes: [u8; 32],
    pn: u32,
    n: u32,
    kem_ct_b64: Option<&str>,
    kem_pub_b64: Option<&str>,
) -> Result<[u8; 32], String> {
    // 1. Out-of-order: a key we already derived and stashed.
    if let Some(mk) = try_skipped(st, dh_hex, n)? {
        return Ok(mk);
    }
    // 2. GLOBAL pre-KDF work bound (must-fix #7). The most keys THIS one message
    //    can force us to derive is (skip the old chain to pn) + (skip the new
    //    chain to n). Because dh_ratchet resets nr to 0, a per-call cap would
    //    allow up to 2*MAX_SKIP; bound the COMBINED total here — before any
    //    kdf_ck runs — so a forged cleartext counter (eph/pn/n are unauthenticated
    //    until the AEAD, which runs later on a clone) can't drive unbounded CPU.
    let new_epoch = st.dhr_pub.as_deref() != Some(dh_hex);
    let old_skip = if new_epoch {
        pn.saturating_sub(st.nr)
    } else {
        0
    };
    let new_start = if new_epoch { 0 } else { st.nr };
    let total_skip = old_skip.saturating_add(n.saturating_sub(new_start));
    if total_skip > MAX_SKIP {
        return Err(format!(
            "ratchet: combined skip {total_skip} exceeds MAX_SKIP {MAX_SKIP}"
        ));
    }
    // 3. A new DH epoch ⇒ finish the previous receiving chain up to pn, turn.
    if new_epoch {
        // Hybrid KEM-ratchet: if the peer already advertised a rolling KEM pub,
        // a hybrid contact MUST carry a KEM ct on every turn — refuse a turn
        // that drops it (downgrade defence; the kem fields are also signed in
        // the sealed header). The accepter's classical first chain never reaches
        // here as a "turn" (the responder turned it at bootstrap), so this can't
        // false-trigger during warm-up.
        let recv_kem_ss = match kem_ct_b64 {
            Some(ct_b64) => {
                let our_kem_priv = st
                    .kem_priv
                    .clone()
                    .ok_or_else(|| "hybrid turn but we hold no rolling KEM private".to_string())?;
                let ct = B64
                    .decode(ct_b64)
                    .map_err(|e| format!("rolling kem_ct b64: {e}"))?;
                let dk = B64
                    .decode(&our_kem_priv)
                    .map_err(|e| format!("rolling kem_priv b64: {e}"))?;
                Some(crypto::ml_kem_decapsulate_local(&ct, &dk)?)
            }
            None => {
                if st.peer_kem_pub.is_some() {
                    return Err(
                        "ratchet: hybrid contact received a classical turn (downgrade refused)"
                            .into(),
                    );
                }
                None
            }
        };
        // Record the peer's freshly-rotated rolling KEM pub BEFORE the turn, so
        // dh_ratchet's new sending chain encapsulates to it.
        if let Some(kp) = kem_pub_b64 {
            st.peer_kem_pub = Some(kp.to_string());
        }
        skip_message_keys(st, pn)?;
        dh_ratchet(st, dh_bytes, recv_kem_ss.as_deref())?;
    }
    // 4. Skip within the current chain to n, then derive the key AT n.
    skip_message_keys(st, n)?;
    let ckr = st
        .ckr
        .clone()
        .ok_or_else(|| "ratchet has no receiving chain".to_string())?;
    let (mk, ckr_next) = crypto::kdf_ck(&b32(&ckr)?);
    st.ckr = Some(hx(&ckr_next));
    st.nr += 1;
    Ok(mk)
}

// ── Attachments (M7): E2E files via the content store ────────────────
//
// Send: encrypt each file under a fresh key (crypto::encrypt_attachment),
// upload the CIPHERTEXT to the content store, and carry the {cid,key,meta}
// Attachment INSIDE the sealed message body. Receive: the Attachment rides in
// the decrypted body; the UI calls fetch_attachment to pull + decrypt on render.
// The store/relay only ever holds opaque ciphertext.

/// Max plaintext size we upload. 64 MiB — raised from 25 MiB now that the app
/// sets android:largeHeap (so the encode/copy buffers fit) and the blobs path
/// streams direct P2P. This is still a SAFETY ceiling, not a policy one: the
/// file is held in RAM a few times (no full streaming yet), so a hard cap
/// prevents an OOM-kill mid-send. Genuinely huge files (full-length video, an
/// APK) need the streaming rework before the cap can be lifted further; until
/// then the caller gets a clean "too large" error instead of a silent stall.
const MAX_ATTACHMENT_BYTES: usize = 64 * 1024 * 1024;
/// Cap for the STREAMED (torrent-style) path. Effectively unlimited for phone use
/// — a 16 GiB sanity bound (the real limit is device disk + the sender staying
/// online). Memory-safe because streaming holds only ONE 256 KiB segment at a time
/// on each side; chunk_count (16 GiB / 256 KiB = 65536) fits u32 with huge margin.
const MAX_STREAMED_ATTACHMENT_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// Ciphertext bytes per content/publish call. The runtime caps the provider
/// body at 2 MB, but EMPIRICALLY its content/publish (IPFS add+pin) gets slow /
/// stalls well before that — a ~1 MB body hung in testing. 256 KiB ciphertext
/// -> ~350 KB body keeps each upload small + fast and well inside what the
/// provider handles when healthy. Larger files split into more chunks.
const ATTACHMENT_CHUNK_BYTES: usize = 256 * 1024;
/// DIRECT (iroh-blobs) transport chunk — MUCH larger than the content-store chunk:
/// iroh-blobs streams over QUIC (no ~2 MB HTTP body limit the content provider has),
/// so bigger chunks mean far fewer round-trips. The ciphertext is reassembled IN
/// ORDER before a single decrypt, so this is purely a transport size.
const BLOB_CHUNK_BYTES: usize = 4 * 1024 * 1024;
/// Blob chunks fetched concurrently (kept in order) so a direct transfer pipelines
/// instead of one-chunk-at-a-time. Bounded so peak RAM stays ~N * chunk.
const BLOB_FETCH_CONCURRENCY: usize = 4;
/// Files at or below this PLAINTEXT size ride INLINE inside the sealed DM
/// instead of the content store: the encrypted blob is base64'd into the
/// `Attachment`, that `Attachment` is part of the DM body which is then sealed
/// AGAIN (the whole envelope is base64'd into JSON), and the resulting wire
/// crosses over the CARRIER, fragmented across the gossip cap by `frag`. Zero
/// IPFS add/pin/DHT-provide, so a small text/doc is instant even when the
/// content provider is slow or unhealthy.
///
/// Fragment cost is dominated by the DOUBLE base64 + the message pad ladder,
/// NOT the raw file size:
///   * a ~5 KB file -> 16 KiB attachment bucket -> the sealed DM body lands in
///     a small message pad bucket -> ~8 fragments (the proven handshake size).
///   * a MAX-size (16000 B) inline file -> 16384 attachment bucket; once it is
///     base64'd into the body and the body is re-sealed, the padded DM
///     plaintext jumps to the 64 KiB message pad bucket and the base64'd
///     envelope wire is ~88 KB -> ~30 fragments at frag CHUNK_BYTES=3000.
/// So the realistic worst case here is ~30 fragments, not ~8. Above this cap
/// (photos, video, big docs) keeps the content-store CID path — inlining a
/// 200 KB photo would be hundreds of gossip messages. 16 KB keeps the padded
/// ATTACHMENT ciphertext in the <=16 KiB attachment bucket (past 16376 it
/// spills to the next bucket); the ~30-fragment DM-wire cost is the real bound
/// and is why this stays small.
const INLINE_ATTACHMENT_MAX_BYTES: usize = 16 * 1000;

/// Encrypt + upload one file; returns the sealed reference to embed in a message.
/// The encrypted blob is split into ≤1 MiB chunks so arbitrarily large files
/// (photos, videos, documents) transport even though each content/publish must
/// stay under the runtime's 2 MB provider body limit. fetch_attachment
/// concatenates the chunks back before decrypt.
pub async fn upload_attachment(name: &str, mime: &str, bytes: &[u8]) -> Result<Attachment, String> {
    if bytes.is_empty() {
        return Err("empty attachment".into());
    }
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "file too large ({} MB; max {} MB)",
            bytes.len() / (1024 * 1024),
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    let (ciphertext, key_b64) = crypto::encrypt_attachment(bytes)?;
    // INLINE fast path: small files ride in the DM body over the carrier with
    // no content-store round-trip. The sealed ciphertext is base64'd into the
    // Attachment; the oversized DM wire is fragmented by `frag` on send and
    // reassembled on receive. Bypasses content/publish entirely, so it is
    // instant and unaffected by content-provider health.
    if bytes.len() <= INLINE_ATTACHMENT_MAX_BYTES {
        return Ok(Attachment {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.chars().take(255).collect(),
            mime: mime.chars().take(128).collect(),
            size: bytes.len() as u64,
            cid: String::new(),
            chunks: Vec::new(),
            tickets: Vec::new(),
            key_b64,
            inline_b64: Some(B64.encode(&ciphertext)),
            ..Default::default()
        });
    }
    // BLOBS FAST PATH (preferred for large files): try iroh-blobs first. Chunk
    // the ciphertext the same way as the content path and add_bytes each chunk
    // to the blobs-provider, collecting one ticket per chunk. The recipient
    // fetches those tickets DIRECTLY P2P from us — no IPFS add/pin/DHT-provide,
    // so it's fast and doesn't touch the content provider's health.
    //
    // LIVENESS TRADEOFF: blobs is direct P2P, so we (the holder) must be ONLINE
    // when the recipient fetches — there is no relay or pin cushion. The
    // content/publish path below is offline-capable (pinned + federated), which
    // is why it is the FALLBACK, not the primary, here. ALL-OR-NOTHING: if the
    // blobs provider is unavailable (NoProvider / unknown-scheme error) OR any
    // single chunk fails to add or yields no ticket, we abandon the blobs
    // attempt entirely and fall through to content/publish. So TODAY — before
    // the blobs-provider is registered — every large upload transparently lands
    // on the content path exactly as it does now.
    if let Some(tickets) = try_upload_blobs(&ciphertext).await {
        return Ok(Attachment {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.chars().take(255).collect(),
            mime: mime.chars().take(128).collect(),
            size: bytes.len() as u64,
            cid: String::new(),
            chunks: Vec::new(),
            tickets,
            key_b64,
            inline_b64: None,
            ..Default::default()
        });
    }
    // FALLBACK — content/publish (CID/chunks). Upload each ciphertext chunk
    // under an opaque filename (the real name is sealed in the message, never
    // handed to the store); pin so the peer can fetch it. Chunk order is
    // preserved = the CID list order. Offline-capable via pinning + federation.
    let mut chunk_cids: Vec<String> = Vec::new();
    for chunk in ciphertext.chunks(ATTACHMENT_CHUNK_BYTES) {
        let resp = crate::runtime::content::add_bytes(chunk, "att.bin", true)
            .await
            .map_err(|e| format!("attachment upload (chunk {}): {e}", chunk_cids.len() + 1))?;
        let cid = crate::runtime::content::extract_cid(&resp)
            .ok_or_else(|| "attachment upload: no cid in response".to_string())?;
        chunk_cids.push(cid);
    }
    let first = chunk_cids.first().cloned().unwrap_or_default();
    Ok(Attachment {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.chars().take(255).collect(),
        mime: mime.chars().take(128).collect(),
        size: bytes.len() as u64,
        cid: first,
        // Only record the list for genuinely multi-chunk blobs; a single-chunk
        // upload stays wire-compatible with the legacy single-`cid` shape.
        chunks: if chunk_cids.len() > 1 { chunk_cids } else { Vec::new() },
        tickets: Vec::new(),
        key_b64,
        inline_b64: None,
        ..Default::default()
    })
}

/// Try to upload the already-encrypted `ciphertext` to the iroh-blobs provider,
/// one ATTACHMENT_CHUNK_BYTES chunk at a time. Returns `Some(tickets)` (one per
/// chunk, in order) ONLY if EVERY chunk added successfully; returns `None` the
/// instant the provider is unavailable or any chunk fails — the caller then
/// falls back to content/publish. Kept all-or-nothing so a half-uploaded blob
/// is never referenced: a mixed cid+ticket attachment can't exist.
async fn try_upload_blobs(ciphertext: &[u8]) -> Option<Vec<String>> {
    let mut tickets: Vec<String> = Vec::new();
    for chunk in ciphertext.chunks(BLOB_CHUNK_BYTES) {
        // Any Err here (NoProvider / unknown scheme / transport) => bail to the
        // content fallback. We do NOT distinguish provider-absent from a
        // transient error: in both cases content/publish is the safe path.
        let resp = crate::runtime::blobs::add_bytes(chunk).await.ok()?;
        let (_hash, ticket) = crate::runtime::blobs::extract_ref(&resp)?;
        tickets.push(ticket);
    }
    // A zero-chunk blob can't happen (callers reject empty bytes), but guard
    // anyway so we never return `Some([])` and mint a ticket-less blobs attachment.
    if tickets.is_empty() {
        return None;
    }
    Some(tickets)
}

/// Streamed (torrent-style) upload for BIG files: read the file at `path` in
/// 256 KiB plaintext segments, encrypt EACH as its own HPC1 frame, and upload each
/// frame to iroh-blobs — only ONE segment is ever in RAM (O(chunk)), so size is
/// bounded by disk, not memory. The recipient fetches + decrypts segment-by-segment
/// to disk. Direct P2P: the sender seeds while online. SMALL files delegate to the
/// bytes path so the inline/threshold logic has a single source of truth.
#[cfg(not(target_arch = "wasm32"))]
pub async fn upload_attachment_streaming(
    path: &str,
    name: &str,
    mime: &str,
) -> Result<Attachment, String> {
    use std::io::Read;
    let size = std::fs::metadata(path).map_err(|e| format!("stat: {e}"))?.len();
    if size == 0 {
        return Err("empty file".into());
    }
    if size > MAX_STREAMED_ATTACHMENT_BYTES {
        return Err(format!(
            "file too large ({} MB; max {} MB)",
            size / (1024 * 1024),
            MAX_STREAMED_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    // SMALL files → the existing bytes path (inline / one-shot / content fallback).
    if size <= ATTACHMENT_CHUNK_BYTES as u64 {
        let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
        return upload_attachment(name, mime, &bytes).await;
    }
    let seg = crypto::ATT_SEG_PLAINTEXT_BYTES;
    let total = (size as usize).div_ceil(seg);
    let total_u32 = u32::try_from(total).map_err(|_| "file has too many chunks".to_string())?;
    let (key_b64, base_nonce) = crypto::begin_streamed_attachment();
    let mut f = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let mut buf = vec![0u8; seg];
    let mut tickets: Vec<String> = Vec::with_capacity(total);
    set_attach_progress("", 0, total_u32);
    for index in 0..total_u32 {
        // Fill up to `seg` bytes (the FS may return short reads).
        let mut filled = 0usize;
        while filled < seg {
            let n = f.read(&mut buf[filled..]).map_err(|e| {
                clear_attach_progress("");
                format!("read (chunk {}/{}): {e}", index + 1, total_u32)
            })?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break; // EOF (shouldn't hit before total, but guard)
        }
        let frame =
            crypto::encrypt_attachment_chunk(&key_b64, &base_nonce, index, total_u32, &buf[..filled])?;
        let resp = crate::runtime::blobs::add_bytes(&frame).await.map_err(|e| {
            clear_attach_progress("");
            format!("blob add (chunk {}/{}): {e}", index + 1, total_u32)
        })?;
        let (_hash, ticket) = crate::runtime::blobs::extract_ref(&resp).ok_or_else(|| {
            clear_attach_progress("");
            "blob add: no ticket".to_string()
        })?;
        tickets.push(ticket);
        set_attach_progress("", index + 1, total_u32);
    }
    clear_attach_progress("");
    if tickets.len() != total {
        return Err(format!(
            "streamed upload chunk mismatch ({} vs {})",
            tickets.len(),
            total
        ));
    }
    Ok(Attachment {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.chars().take(255).collect(),
        mime: mime.chars().take(128).collect(),
        size,
        tickets,
        key_b64,
        streamed: true,
        chunk_count: total_u32,
        base_nonce_b64: Some(B64.encode(base_nonce)),
        ..Default::default()
    })
}

#[cfg(target_arch = "wasm32")]
pub async fn upload_attachment_streaming(
    _path: &str,
    _name: &str,
    _mime: &str,
) -> Result<Attachment, String> {
    Err("streaming upload not supported on this platform".into())
}

/// Fetch + decrypt one attachment's plaintext bytes (render path, both sides).
/// Reassembles the ciphertext from its chunk CIDs (or the single legacy `cid`)
/// before decrypting — each chunk fetch is an independent content/IPFS get.
// ── attachment download progress (chunk-based; powers the receiver's % UI) ──
fn attach_progress_cell() -> &'static std::sync::Mutex<(u32, u32)> {
    static C: std::sync::OnceLock<std::sync::Mutex<(u32, u32)>> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new((0, 0)))
}
fn set_attach_progress(_id: &str, done: u32, total: u32) {
    if let Ok(mut c) = attach_progress_cell().lock() {
        *c = (done, total);
    }
}
fn clear_attach_progress(_id: &str) {
    if let Ok(mut c) = attach_progress_cell().lock() {
        *c = (0, 0);
    }
}
/// Download progress (0..=100) for the CURRENT in-flight chunked attachment fetch,
/// or -1 when none active. GLOBAL (the UI fetches one at a time) so the % never
/// depends on matching an attachment id across the JNI boundary — that matching
/// was the suspected cause of the missing %.
pub fn attachment_progress(_id: &str) -> i32 {
    attach_progress_cell()
        .lock()
        .ok()
        .map(|c| {
            let (d, t) = *c;
            if t == 0 {
                -1
            } else {
                ((d as u64 * 100) / t as u64) as i32
            }
        })
        .unwrap_or(-1)
}

/// Max ciphertext chunks/tickets we will accept for ONE attachment, derived from
/// the existing size ceilings + minimum sane chunk size — far above any legit
/// transfer. A whole-file (non-streamed) attachment is bounded by
/// MAX_ATTACHMENT_BYTES; the smallest chunk unit is the content-store chunk.
/// (16 GiB streamed / 256 KiB segment = 65536; +slack.)
const MAX_ATTACHMENT_CHUNKS: usize = 70_000;

/// Validate an attachment's declared size + chunk/ticket COUNT BEFORE we start
/// accumulating any ciphertext (anti-DoS: a forged ref must not let us allocate
/// unbounded RAM or issue unbounded fetches). Uses the EXISTING size ceilings so
/// legit large transfers are unaffected. `streamed` picks the higher ceiling.
fn validate_attachment_bounds(att: &Attachment, streamed: bool) -> Result<(), String> {
    let ceiling: u64 = if streamed {
        MAX_STREAMED_ATTACHMENT_BYTES
    } else {
        MAX_ATTACHMENT_BYTES as u64
    };
    if att.size > ceiling {
        return Err(format!(
            "attachment too large ({} bytes > {} ceiling)",
            att.size, ceiling
        ));
    }
    let n = att.chunks.len().max(att.tickets.len());
    if n > MAX_ATTACHMENT_CHUNKS {
        return Err(format!(
            "attachment chunk/ticket count {} exceeds cap {}",
            n, MAX_ATTACHMENT_CHUNKS
        ));
    }
    Ok(())
}

pub async fn fetch_attachment(att: &Attachment) -> Result<Vec<u8>, String> {
    // SAFETY BOUNDS (anti-DoS): validate declared size + chunk/ticket count BEFORE
    // accumulating any ciphertext. Whole-file path → MAX_ATTACHMENT_BYTES ceiling.
    validate_attachment_bounds(att, false)?;
    // INLINE attachment: the ciphertext rode in the DM body — decode + decrypt
    // with no content-store fetch (the bytes never touched IPFS).
    if let Some(b64) = &att.inline_b64 {
        let ciphertext = B64
            .decode(b64)
            .map_err(|e| format!("inline attachment b64: {e}"))?;
        let plaintext = crypto::decrypt_attachment(&ciphertext, &att.key_b64)?;
        if plaintext.len() as u64 != att.size {
            return Err(format!(
                "attachment size mismatch (sealed {}, decrypted {})",
                att.size,
                plaintext.len()
            ));
        }
        return Ok(plaintext);
    }
    // BLOBS attachment: the ciphertext lives in iroh-blobs, one ticket per
    // chunk. Fetch each ticket DIRECTLY P2P from the holder (the sender must be
    // online — direct P2P has no relay/pin cushion), concatenate in order, then
    // decrypt + size-check exactly like the content path. Checked before `cid`
    // so a blobs attachment is never misread as a (missing) content CID.
    if !att.tickets.is_empty() {
        let total = att.tickets.len() as u32;
        set_attach_progress(&att.id, 0, total);
        // RUNNING cumulative-byte ceiling: a forged ref could declare a small size
        // yet serve huge chunks — abort BEFORE OOM, not after. Bounded by the
        // whole-file ceiling + AEAD overhead slack (legit transfers stay under it).
        let cipher_ceiling = att.cipher_fetch_ceiling();
        let mut ciphertext: Vec<u8> = Vec::new();
        // Fetch chunks CONCURRENTLY but reassemble IN ORDER: `buffered` yields
        // completed futures in their original order, so a direct P2P transfer
        // pipelines (multiple QUIC streams to the holder) instead of one-at-a-time.
        use futures_util::StreamExt;
        let mut parts = futures_util::stream::iter(att.tickets.iter().cloned())
            .map(|ticket| async move { crate::runtime::blobs::fetch_bytes(&ticket).await })
            .buffered(BLOB_FETCH_CONCURRENCY);
        let mut done: u32 = 0;
        while let Some(res) = parts.next().await {
            let part = res.map_err(|e| {
                clear_attach_progress(&att.id);
                format!("attachment blobs fetch (chunk {}/{}): {e}", done + 1, total)
            })?;
            ciphertext.extend_from_slice(&part);
            if ciphertext.len() as u64 > cipher_ceiling {
                clear_attach_progress(&att.id);
                return Err("attachment exceeds size ceiling mid-fetch (rejected)".into());
            }
            done += 1;
            set_attach_progress(&att.id, done, total);
        }
        clear_attach_progress(&att.id);
        let plaintext = crypto::decrypt_attachment(&ciphertext, &att.key_b64)?;
        if plaintext.len() as u64 != att.size {
            return Err(format!(
                "attachment size mismatch (sealed {}, decrypted {})",
                att.size,
                plaintext.len()
            ));
        }
        return Ok(plaintext);
    }
    let cids: Vec<String> = if att.chunks.is_empty() {
        vec![att.cid.clone()]
    } else {
        att.chunks.clone()
    };
    let total = cids.len() as u32;
    set_attach_progress(&att.id, 0, total);
    // RUNNING cumulative-byte ceiling (see the blobs path above) — abort a forged
    // oversized transfer BEFORE OOM rather than after.
    let cipher_ceiling = att.cipher_fetch_ceiling();
    let mut ciphertext: Vec<u8> = Vec::new();
    for (i, cid) in cids.iter().enumerate() {
        let part = crate::runtime::content::get_bytes(cid, None)
            .await
            .map_err(|e| {
                clear_attach_progress(&att.id);
                format!("attachment fetch (chunk {}/{}): {e}", i + 1, cids.len())
            })?;
        ciphertext.extend_from_slice(&part);
        if ciphertext.len() as u64 > cipher_ceiling {
            clear_attach_progress(&att.id);
            return Err("attachment exceeds size ceiling mid-fetch (rejected)".into());
        }
        set_attach_progress(&att.id, (i + 1) as u32, total);
    }
    clear_attach_progress(&att.id);
    let plaintext = crypto::decrypt_attachment(&ciphertext, &att.key_b64)?;
    // The sealed `size` is the real length; cross-check it against the unpadded
    // bytes so a truncated/padded-mismatched blob is caught, not silently served.
    if plaintext.len() as u64 != att.size {
        return Err(format!(
            "attachment size mismatch (sealed {}, decrypted {})",
            att.size,
            plaintext.len()
        ));
    }
    Ok(plaintext)
}

/// Streamed (torrent-style) fetch: download each HPC1 segment, decrypt it, and
/// APPEND to `dest` — only ONE segment in RAM at a time (O(chunk)), so receive is
/// memory-safe for any size. Non-streamed attachments delegate to fetch_attachment
/// (small / whole-buffer). HARD-asserts the segment count + total size, so a
/// truncated transfer errors instead of writing a partial file.
#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_attachment_to_path(att: &Attachment, dest: &str) -> Result<(), String> {
    use std::io::{Seek, SeekFrom, Write};
    if !att.streamed {
        let bytes = fetch_attachment(att).await?;
        std::fs::write(dest, &bytes).map_err(|e| format!("write: {e}"))?;
        return Ok(());
    }
    // SAFETY BOUNDS (anti-DoS): validate declared size + ticket count BEFORE we
    // start fetching. Streamed path → the higher MAX_STREAMED ceiling so genuinely
    // huge (but legit) transfers are unaffected; only forged refs are rejected.
    validate_attachment_bounds(att, true)?;
    let base_b64 = att
        .base_nonce_b64
        .as_deref()
        .ok_or("streamed attachment missing base nonce")?;
    let base_vec = B64
        .decode(base_b64)
        .map_err(|e| format!("base nonce b64: {e}"))?;
    let base_nonce: [u8; 12] = base_vec
        .as_slice()
        .try_into()
        .map_err(|_| "base nonce must be 12 bytes".to_string())?;
    let total = att.chunk_count;
    // The sealed chunk_count is trustworthy; require the ticket list to match it
    // so a stripped-tickets attachment can't smuggle a short file past us.
    if att.tickets.len() as u32 != total {
        return Err(format!(
            "streamed ticket/count mismatch ({} vs {})",
            att.tickets.len(),
            total
        ));
    }
    // RESUME: skip chunks already on disk so a stalled download CONTINUES from where
    // it stopped instead of restarting at 0. A clean boundary = full_chunks * seg;
    // any partial tail from a mid-chunk abort is discarded + refetched. The caller
    // passes a STABLE dest per attachment so a re-tap resumes.
    let seg = crypto::ATT_SEG_PLAINTEXT_BYTES as u64;
    let existing = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if existing == att.size {
        // already fully fetched (cached) → instant.
        set_attach_progress("", total, total);
        clear_attach_progress("");
        return Ok(());
    }
    let mut completed = if existing > att.size { 0u32 } else { (existing / seg) as u32 };
    if completed > total {
        completed = 0;
    }
    let resume_at = completed as u64 * seg;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(dest)
        .map_err(|e| format!("open dest: {e}"))?;
    out.set_len(resume_at).map_err(|e| format!("truncate: {e}"))?; // drop any partial tail
    out.seek(SeekFrom::Start(resume_at)).map_err(|e| format!("seek: {e}"))?;
    let mut written: u64 = resume_at;
    set_attach_progress("", completed, total);
    for i in completed..total {
        let ticket = &att.tickets[i as usize];
        let frame = crate::runtime::blobs::fetch_bytes(ticket).await.map_err(|e| {
            clear_attach_progress("");
            format!("blob fetch (chunk {}/{}): {e}", i + 1, total)
        })?;
        let pt = crypto::decrypt_attachment_chunk(&att.key_b64, &base_nonce, i, total, &frame)
            .map_err(|e| {
                clear_attach_progress("");
                e
            })?;
        out.write_all(&pt).map_err(|e| {
            clear_attach_progress("");
            format!("write: {e}")
        })?;
        written += pt.len() as u64;
        set_attach_progress("", i + 1, total);
    }
    clear_attach_progress("");
    out.flush().map_err(|e| format!("flush: {e}"))?;
    // Truncation/size guard against the SEALED size (decrypt already auth-bound
    // each segment to its index+total). A mismatch deletes the partial file.
    if written != att.size {
        drop(out);
        let _ = std::fs::remove_file(dest);
        return Err(format!(
            "streamed size mismatch (wrote {}, sealed {})",
            written, att.size
        ));
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_attachment_to_path(_att: &Attachment, _dest: &str) -> Result<(), String> {
    Err("streaming fetch not supported on this platform".into())
}

/// Parse the `attachments` array out of a decrypted inner-payload body.
fn attachments_from_body(body: &Value) -> Vec<Attachment> {
    body.get("attachments")
        .and_then(|v| serde_json::from_value::<Vec<Attachment>>(v.clone()).ok())
        .unwrap_or_default()
}

// ── Public send / receive entry points ───────────────────────────────

/// Build the wire string for a ratchet message: advance the sending chain,
/// PERSIST the advanced state before `mk` is used (so a crash can never reuse
/// it — must-fix #5), then seal the signed inner payload under `mk` + a fresh
/// ML-KEM encapsulation to the peer's STATIC kem key. The cleartext `rh`
/// carries the page number (`pn`,`n`); the sealed `InnerPayload.rh` carries the
/// same triple under the signature.
#[allow(clippy::too_many_arguments)]
async fn build_ratchet_wire(
    peer_did: &str,
    peer_keys: &PeerKeys,
    body: &Value,
    my_did: &str,
    my_seed_hex: &str,
    bind_recipient: Option<&str>,
    bind_conv: Option<&str>,
) -> Result<String, String> {
    let mut st = read_ratchet(peer_did)
        .await
        .filter(|s| s.is_bootstrapped())
        .ok_or_else(|| {
            "ratchet-capable contact has no ratchet state (refusing to downgrade)".to_string()
        })?;
    let (mk, header) = ratchet_step_send(&mut st)?;
    // Wipe the transient message key from the heap when this scope ends (L:
    // transient AEAD key not zeroized). `Zeroizing<[u8;32]>` derefs to `[u8;32]`,
    // so `&mk` below is unchanged and no ciphertext/derivation output differs.
    let mk = zeroize::Zeroizing::new(mk);
    write_ratchet(peer_did, &st)
        .await
        .map_err(|e| e.to_string())?;
    let inner = build_inner_bound(
        KIND_MESSAGE,
        body,
        my_did,
        my_seed_hex,
        Some(header.clone()),
        bind_recipient,
        bind_conv,
    )
    .await?;
    let plaintext = serde_json::to_string(&inner).map_err(|e| format!("inner json: {e}"))?;
    let recipient_kem = B64
        .decode(&peer_keys.ml_kem_pub_b64)
        .map_err(|e| format!("peer ml-kem b64: {e}"))?;
    let dhs_pub: [u8; 32] = B64
        .decode(&header.dh)
        .map_err(|e| format!("ratchet dh b64: {e}"))?
        .try_into()
        .map_err(|_| "ratchet dh wrong size".to_string())?;
    let envelope = crypto::encrypt_with_mk(&plaintext, &mk, &recipient_kem, &dhs_pub)?;
    // Cleartext page header — the receiver needs `kc`/`kp` BEFORE decrypt to turn
    // the KEM ratchet. They duplicate the sealed+signed `RatchetHeader.kem_*`, so
    // tampering either fails the AEAD (wrong derived key) or the post-decrypt
    // header-equality check.
    let mut rh = json!({ "pn": header.pn, "n": header.n });
    if let Some(kc) = &header.kem_ct {
        rh["kc"] = json!(kc);
    }
    if let Some(kp) = &header.kem_pub {
        rh["kp"] = json!(kp);
    }
    Ok(json!({
        "type": "dm.v2",
        "rh": rh,
        "envelope": envelope,
    })
    .to_string())
}

/// Send a message. v2 path (sealed sender, per-pair queue) is used when
/// the contact is is_v2_active(); otherwise we fall through to the
/// legacy v1 path for back-compat with contacts created before queues.
pub async fn send_message(peer_did: &str, text: &str) -> Result<DmMessage, String> {
    send_message_inner(peer_did, text, Vec::new(), None).await
}

/// Send a 1:1 message that QUOTES another message (tap-to-reply). `reply` carries the
/// quoted id/author/snippet; it rides inside the sealed body and is stored on the message.
pub async fn send_message_reply(
    peer_did: &str,
    text: &str,
    reply: ReplyRef,
) -> Result<DmMessage, String> {
    send_message_inner(peer_did, text, Vec::new(), Some(reply)).await
}

/// Send a message carrying E2E attachments. Upload each file with
/// `upload_attachment` first, then pass the refs here. Attachments require a
/// metadata-safe (v2) contact.
pub async fn send_message_with_attachments(
    peer_did: &str,
    text: &str,
    attachments: Vec<Attachment>,
) -> Result<DmMessage, String> {
    send_message_inner(peer_did, text, attachments, None).await
}

async fn send_message_inner(
    peer_did: &str,
    text: &str,
    attachments: Vec<Attachment>,
    reply: Option<ReplyRef>,
) -> Result<DmMessage, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() && attachments.is_empty() {
        return Err("empty message".into());
    }
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    if peer_did == me.did_key {
        return Err("cannot DM yourself".into());
    }

    let plain_text: String = trimmed.chars().take(4096).collect();
    // CHAT-CAPABILITY ISOLATION: a USER message (not a SOH-prefixed control DM like the feed key)
    // may only be sent to a chat-ENABLED contact. A follow-only contact — bootstrapped by a
    // follow/accept solely to carry the feed key, never established via a chat QR/invite — fails
    // closed here, so FOLLOWING someone can NEVER open a private chat. Refuse BEFORE the local echo
    // so no phantom thread is created. Control DMs (feed key, follow/addr/call signals …) all start
    // with \u{1} and bypass this gate, so feed-key delivery over the follow contact still works.
    if !plain_text.starts_with('\u{1}') && !is_chat_enabled(peer_did).await {
        return Err("chat not enabled — scan their chat QR to start a private chat".into());
    }
    let contact = find_contact(peer_did).await;

    // ── LOCAL ECHO FIRST (Fix 1) ──────────────────────────────────────────────
    // Persist the outgoing message BEFORE any transport-gating early-return
    // (PendingInvite / needs_verify_before_send). Those gates still BLOCK the
    // actual send below, but the user's message must never silently vanish — it
    // is echoed into the conversation here so it survives even when the send is
    // gated. `use_v2`/`legacy_encrypted` only READ contact/keys (no side
    // effects), so they are safe to compute up front to feed `msg.encrypted`.
    let use_v2 = contact.as_ref().map(|c| c.is_v2_active()).unwrap_or(false);
    // Local-side message (mine=true), always plaintext on disk. The
    // `encrypted` flag is for our own UI hint; v2 path is always
    // encrypted; legacy v1 is encrypted iff we've cached peer keys.
    let legacy_encrypted = !use_v2 && get_peer_keys(peer_did).await.is_some();
    let msg = DmMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: plain_text.clone(),
        ts: now_ms(),
        mine: true,
        encrypted: use_v2 || legacy_encrypted,
        attachments: attachments.clone(),
        sender_name: String::new(),
        // Our own outgoing message — author is us (verified by construction).
        sender_did: me.did_key.clone(),
        pinned: false,
        reply_to: reply.clone(),
    };
    let preview = if plain_text.is_empty() && !attachments.is_empty() {
        format!("📎 {}", attachments[0].name)
    } else {
        plain_text.clone()
    };
    let mut conv = read_conversation(peer_did).await;
    conv.push(msg.clone());
    write_conversation(peer_did, &conv)
        .await
        .map_err(|e| e.to_string())?;
    // Hidden control messages (SOH-prefixed: address card, feed-key handshake, call/follow/edit/
    // delete/verse signals) are protocol traffic. Store them (their handler reads them back) but
    // NEVER bump the chat-list preview / last_ts — mirroring the receive path's is_hidden_ctrl gate.
    // Without this, an OUTBOUND control DM (e.g. the feed-key the author sends on accept/chat-open)
    // shows up as the latest "message" in the chat list (the `\u{1}hey-social.feed_key:` leak).
    if !plain_text.starts_with('\u{1}') {
        touch_contact_message(peer_did, &preview, msg.ts, 0)
            .await
            .map_err(|e| e.to_string())?;
    }

    // PendingInvite — they haven't replied to our invite yet. The message is
    // already echoed locally above; this only blocks the transport send.
    if let Some(c) = &contact {
        if c.status == ContactStatus::PendingInvite {
            return Err("Awaiting their invite acceptance — they haven't replied yet.".into());
        }
        // F-FOLLOW-PoP: this contact's keys came from an UNVERIFIED, UNSIGNED
        // source and we've never sealed to them. Block the FIRST *user-content*
        // send with a STABLE sentinel error the UI layer keys on to prompt the
        // user to verify the safety number (verify_contact) or send anyway
        // (confirm_unverified_send). Both clear the gate; subsequent sends flow
        // normally. Existing contacts with history are never flagged, so this
        // never blocks an established chat. Hidden CONTROL messages (SOH-prefixed:
        // the follow-announce key bundle, address card, call signals, tombstones,
        // verse) are protocol traffic the user already consented to by taking the
        // action — they are NOT gated, so they still flow (the announce itself is
        // what lets the peer DM us back).
        if c.needs_verify_before_send && !trimmed.starts_with('\u{1}') {
            crate::plat::warn(&format!("[hey-core] send GATED needs_verify did={} key_verified={} key_changed={} last_ts={} v2={}", peer_did, c.key_verified, c.key_changed, c.last_ts, c.is_v2_active()));
            return Err(NEEDS_VERIFY_BEFORE_SEND.into());
        }
    }

    if !attachments.is_empty() && !use_v2 {
        return Err("attachments need a metadata-safe (v2) contact".into());
    }

    if use_v2 {
        // Carry MY current nickname so the receiver shows it even for a contact who
        // has never posted / isn't followed — the 1:1 sibling of the group "sn".
        // Mode-gated: an Anonymous-mode contact gets sn="" (suppressed by the
        // receiver's `.filter(|s| !s.is_empty())`), so the real persona name never
        // rides to an incognito peer — matching invite/handshake/broadcast behavior.
        let real_name = ensure_profile().await.map(|m| m.name).unwrap_or_default();
        let my_name = contact
            .as_ref()
            .map(|c| shared_display_name(c.mode, &real_name))
            .unwrap_or(real_name);
        let mut body = if attachments.is_empty() {
            json!({ "text": plain_text, "mid": msg.id, "sn": my_name })
        } else {
            json!({ "text": plain_text, "attachments": attachments, "mid": msg.id, "sn": my_name })
        };
        if let Some(r) = &reply {
            body["reply"] = reply_ref_json(r);
        }
        send_body_to_contact(peer_did, &body).await?;
        return Ok(msg);
    }

    // No v2 contact ⇒ nothing to send (the legacy v1 plaintext/seed path is
    // gone; new conversations are always bootstrapped through invite links).
    Err("contact is not metadata-safe (v2) — re-invite to establish a channel".into())
}

/// Seal `body` to a single v2 contact and publish it on the per-pair queue —
/// the shared wire-build + publish step for BOTH 1-to-1 DMs and group fan-out,
/// so the queue/ratchet/sealed-sender choice can never diverge between them.
/// Does NOT touch local conversation state (the caller owns that). `body` is the
/// inner-payload body (`{text[, attachments][, group]}`); group fan-out passes
/// the same body to each member with a `group` field added.
async fn send_body_to_contact(peer_did: &str, body: &Value) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let s = session::current().ok_or_else(|| "not signed in".to_string())?;
    let c = find_contact(peer_did)
        .await
        .filter(|c| c.is_v2_active())
        .ok_or_else(|| "not a metadata-safe (v2) contact".to_string())?;

    // Regular-mode contacts: DETERMINISTIC per-pair queue (both peers derive the
    // SAME q/<id> from real DIDs) so the send topic always matches the
    // recipient's listen topic. Anonymous contacts keep the advertised minted
    // queue (peer can't derive without the real DID).
    //
    // F-11 MIGRATION: once the peer has advertised salted support (`peer_salted`)
    // AND we can derive the salted topic, we MIGRATE this send to the salted
    // topic (HKDF over the per-pair X25519 secret — not computable by a DID-only
    // observer). Until then we send on the legacy deterministic topic, which is
    // ALWAYS deliverable. The peer keeps listening on BOTH topics (see
    // my_v2_topics), so a send on either is never stranded — the grace overlap.
    let queue = if matches!(c.mode, IdentityMode::Regular) {
        let legacy = pair_inbound_queue(peer_did, &me.did_key);
        if c.peer_salted {
            ensure_salted_queue(peer_did).await.unwrap_or(legacy)
        } else {
            legacy
        }
    } else if let Some(a) = c.anon_identity.as_ref() {
        // INCOGNITO (anon-DM cross-runtime fix): send on the PRIVATE deterministic queue derived
        // from OUR EPHEMERAL did — which is EXACTLY the queue the (regular) peer already listens on
        // for us (it treats us as an ordinary contact keyed by our ephemeral did, so it computes
        // pair_inbound_queue(our_ephemeral, peer)). This gives incognito the SAME self-healing
        // convergence regular chats have, with NO dependency on the minted-queue/welcome rotation
        // (the bug: the peer sent here while we only listened on the rotated minted queue → drop).
        // Uses the throwaway ephemeral did ONLY — never the real DID, so incognito stays anonymous.
        pair_inbound_queue(peer_did, &a.did)
    } else {
        c.their_inbound_queue
            .clone()
            .ok_or_else(|| "no inbound queue for this contact yet".to_string())?
    };
    let send_pseudonym = c
        .my_send_pseudonym
        .clone()
        .unwrap_or_else(|| "anonymous".into());
    let peer_keys = c
        .peer_pubkeys
        .clone()
        .ok_or_else(|| "no peer keys for this contact".to_string())?;
    // Sign as the identity this contact knows us by (real DID in Regular mode,
    // the per-contact ephemeral DID in Anonymous mode).
    let (my_did, my_seed_hex) =
        signing_identity(c.mode, c.anon_identity.as_ref(), &me.did_key, &s.auth_key_hex)?;

    // F-08: bind the inner signature to the recipient + conversation so a mutual
    // contact can't re-seal this message into a DIFFERENT conversation. Only for
    // Regular-mode contacts, where BOTH sides reconstruct the SAME pair: the
    // receiver's real DID (= `peer_did` here, = their `my_did` on receipt) and
    // the deterministic per-pair `queue`. Anonymous contacts stay on the legacy
    // form (no symmetric real DID; minted queues already gate cross-conversation
    // reuse). The verifier falls back to the legacy form regardless, so this
    // never breaks delivery to/from a not-yet-upgraded peer.
    let (bind_recipient, bind_conv): (Option<&str>, Option<&str>) =
        if matches!(c.mode, IdentityMode::Regular) {
            (Some(peer_did), Some(queue.as_str()))
        } else {
            (None, None)
        };

    // F-11: advertise OUR salted-topic support inside the sealed+signed body
    // (`sc:true`) so the peer learns it can migrate its sends to the salted
    // topic. We only advertise it when WE can derive (and therefore listen on)
    // the salted topic — so the peer never migrates sends onto a topic we don't
    // join. Anonymous contacts have no legacy pair-topic leak to fix, so skip.
    // Adding a body field is signed consistently (the whole body is in the signed
    // set) and unknown fields are ignored by every receiver — backward-compat.
    let body_owned: Value;
    let body: &Value = if matches!(c.mode, IdentityMode::Regular)
        && body.is_object()
        && ensure_salted_queue(peer_did).await.is_some()
    {
        let mut b = body.clone();
        b["sc"] = Value::Bool(true);
        body_owned = b;
        &body_owned
    } else {
        body
    };

    // Ratchet-capable contacts ALWAYS ratchet (no silent downgrade — must-fix
    // #6); others use the single-shot seal to static keys.
    let wire = if c.ratchet_capable {
        build_ratchet_wire(
            peer_did,
            &peer_keys,
            body,
            &my_did,
            &my_seed_hex,
            bind_recipient,
            bind_conv,
        )
        .await?
    } else {
        let inner = build_inner_bound(
            KIND_MESSAGE,
            body,
            &my_did,
            &my_seed_hex,
            None,
            bind_recipient,
            bind_conv,
        )
        .await?;
        let envelope = encrypt_inner_for_peer(&inner, &peer_keys)?;
        json!({ "type": "dm.v2", "envelope": envelope }).to_string()
    };

    let topic = format!("{TOPIC_PREFIX_V2}/{queue}");
    // Seed the mesh to the peer's runtime so the send reaches their queue.
    // R6-TICKET-POISON (deferred): a non-self-asserted peer_ticket can be owner-poisoned
    // (id=Bob, addrs=[eve-relay]) so our dial routes through the attacker's relay (a metadata/
    // source-IP leak — content stays sealed). The correct fix STRIPS the relay/addr hints but
    // KEEPS the authenticated EndpointId (iroh still resolves the peer via pkarr/DNS discovery),
    // applied uniformly at every dial site (this send boot, my_v2_topics receive boot, and the
    // transport-badge poll). Simply dropping the ticket for non-self-asserted contacts breaks
    // delivery to brand-new/discovery-only contacts (no boot, no other neighbor yet), so it is
    // deferred to that addrs-strip helper rather than shipped as a delivery regression. LOW sev.
    let boot: Vec<String> = c.peer_ticket.iter().cloned().collect();
    let _ = peer::join_topic_with(&topic, &boot).await;
    let _ = crate::api::outbox::publish_or_enqueue(&topic, &boot, &send_pseudonym, &wire).await;
    Ok(())
}

// ── Group chat (pairwise fan-out) ─────────────────────────────────────
//
// A group message reuses the ENTIRE 1-to-1 machinery: it is sent as an
// individual sealed-sender (+ Double Ratchet where available) message to EACH
// member over their per-pair queue, tagged with a `group` context. There is NO
// group key — every link keeps its own PQ + forward-secrecy, and the relay sees
// only opaque per-pair traffic. Members must be mutual v2 contacts (each member
// fans out to every other, so the pairwise channel must already exist); a group
// built from established contacts satisfies this. The `group` context (id + name
// + roster) rides in every message, so a member who never saw an explicit invite
// still materialises the group on first receipt.

const GROUPS_FILE: &str = "dm/groups.json";

/// A signed role assignment. ONLY the owner (`created_by`) may issue one, and it
/// carries its OWN issuer Ed25519 signature (`sig`) so a member re-broadcasting
/// the roster ctx cannot forge authority — the outer InnerPayload.sig only proves
/// who SENT the wire (adversarial must-fix #7). `role`: 0 = member (demote),
/// 1 = admin (promote); role 2 (owner) is ALWAYS rejected — ownership is
/// immutable. `epoch` is monotone (anti-rollback); `nonce` makes replays distinct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleGrant {
    pub gid: String,
    pub subject: String,
    pub role: u8,
    pub epoch: u64,
    pub issuer: String,
    pub nonce: String,
    pub sig: String,
}

/// Removal tombstone for a kicked/blocked member, propagated so a STALE larger
/// roster can't silently undo a kick (the kick/block barrier, must-fix #5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovedMember {
    pub did: String,
    pub epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub did: String,
    #[serde(default)]
    pub name: String,
    /// The member's X25519 + ML-KEM pubkeys, carried in the roster so a
    /// recipient can bootstrap a pairwise channel to a member it doesn't already
    /// know (the 3+ member fan-out fix). TOFU-via-creator trust: the creator
    /// vouches for these (they had the member as a verified v2 contact); we never
    /// OVERWRITE an existing verified contact with roster keys.
    #[serde(default, rename = "peerPubkeys", skip_serializing_if = "Option::is_none")]
    pub peer_pubkeys: Option<PeerKeys>,
    /// The member's gossip node ticket, so the bootstrapped contact's
    /// deterministic pair-queue meshes cross-runtime.
    #[serde(default, rename = "peerTicket", skip_serializing_if = "Option::is_none")]
    pub peer_ticket: Option<String>,
    /// F-ROSTER-KEYPOISON: the MEMBER's OWN Ed25519 proof-of-possession over
    /// `canonical_member_pop(did, peer_pubkeys)` — a self-signature binding these
    /// PQ keys to this member's did:key. The owner BUILDS the roster from its
    /// contacts and cannot forge another member's signature, so a malicious owner
    /// cannot pin attacker keys under a co-member's DID as a SEALING key: a roster
    /// entry whose `key_pop` is ABSENT or INVALID is pinned discovery-only
    /// (`key_verified=false`, never the sealing key for a fresh contact). A valid
    /// PoP lets the keys be pinned (still unverified — the member's own later
    /// follow/invite/handshake upgrades to verified). Ticket-independent so it
    /// survives ticket rotation. serde-default ⇒ legacy rosters (no field) load and
    /// fall through to the discovery-only path.
    #[serde(default, rename = "keyPop", skip_serializing_if = "Option::is_none")]
    pub key_pop: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub members: Vec<GroupMember>,
    #[serde(default, rename = "lastTs")]
    pub last_ts: i64,
    #[serde(default, rename = "lastPreview")]
    pub last_preview: String,
    #[serde(default)]
    pub unread: u32,
    /// DID of the member who created the group (owner). Empty for legacy groups
    /// created before this field. Gates owner-only actions (set_group_meta).
    #[serde(default, rename = "createdBy")]
    pub created_by: String,
    /// Optional group bio + avatar (CID of an IPFS-pinned image). Owner-set,
    /// propagated in the roster ctx.
    #[serde(default)]
    pub bio: String,
    #[serde(default, rename = "avatarCid", skip_serializing_if = "Option::is_none")]
    pub avatar_cid: Option<String>,
    /// True while this group is awaiting the user's accept (join-consent): a
    /// received roster materialises a PENDING group instead of auto-joining.
    /// accept_group flips it false; decline_group drops it.
    #[serde(default)]
    pub pending: bool,
    /// True once the creator has DISSOLVED the group for everyone (admin
    /// "delete group for everyone"). A closed group can never be posted to and
    /// surfaces as read-only/archived in the UI. Defaults false (back-compat:
    /// legacy groups load as open).
    #[serde(default)]
    pub closed: bool,
    // ── governance (admin features) — all serde-default so legacy groups load ──
    /// Materialized admin DIDs, DERIVED by applying `grants` (cached for the UI).
    /// The owner (`created_by`) outranks all admins and is NOT listed here.
    #[serde(default)]
    pub admins: Vec<String>,
    /// Kicked-and-barred DIDs: dropped from the roster AND barred from rejoining
    /// via a stale roster (the kick/block barrier).
    #[serde(default)]
    pub blocked: Vec<String>,
    /// Muted DIDs: messages are STORED but never notify/bump-unread (enforced for
    /// everyone — soft, reversible moderation).
    #[serde(default)]
    pub muted: Vec<String>,
    /// Monotone governance epoch. Every owner/admin op bumps it; a received ctx
    /// with epoch ≤ the one already applied is rejected (anti-rollback, #2).
    #[serde(default)]
    pub epoch: u64,
    /// Owner-signed role grants — the source of truth for `admins`, propagated in
    /// the ctx so every replica verifies authority independently.
    #[serde(default)]
    pub grants: Vec<RoleGrant>,
    /// Removal tombstones (kick/block), propagated so a stale roster can't undo a kick.
    #[serde(default)]
    pub removed: Vec<RemovedMember>,
}

fn group_conv_path(gid: &str) -> String {
    let safe: String = gid.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    format!("dm/group-conv/{safe}.json")
}

async fn read_groups() -> Vec<Group> {
    storage::read_json(GROUPS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}
async fn write_groups(list: &[Group]) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(list).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(GROUPS_FILE, &v).await
}

/// All groups this device belongs to, most-recently-active first.
pub async fn list_groups() -> Vec<Group> {
    let mut g = read_groups().await;
    g.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    g
}

/// One group by id, or None if this device isn't in it. Exposed so the native
/// group-call roster builder can authorize participants against the
/// owner-controlled roster (members + kick/block bars) instead of trusting the
/// self-asserted call payload (F-GCALL-ROSTER).
pub async fn find_group(gid: &str) -> Option<Group> {
    read_groups().await.into_iter().find(|g| g.id == gid)
}

/// True iff `did` is a CURRENT, non-barred member (or the owner) of this group —
/// the same authority test the group-message membership gate uses, surfaced for
/// the call-roster filter. A kicked/blocked DID (still listed by a stale roster)
/// returns false. `g.members`/`is_group_barred` are evaluated at read time, so a
/// member kicked mid-call drops from the roster on the next poll.
pub fn group_member_authorized(g: &Group, did: &str) -> bool {
    if did.is_empty() {
        return false;
    }
    let is_member = g.created_by == did || g.members.iter().any(|m| m.did == did);
    is_member && !is_group_barred(g, did)
}

/// The roster-pinned peer node ticket for `did` in this group, if the
/// owner-built roster carried one. Used to confirm a call payload's ticket
/// resolves to this member's KNOWN endpoint before it's spliced into the audio/
/// video mesh (F-GCALL-ROSTER). None when the roster has no ticket for them.
pub fn group_member_peer_ticket(g: &Group, did: &str) -> Option<String> {
    g.members
        .iter()
        .find(|m| m.did == did)
        .and_then(|m| m.peer_ticket.clone())
}

/// The message log for a group (oldest first).
pub async fn read_group_conversation(gid: &str) -> Vec<DmMessage> {
    storage::read_json(&group_conv_path(gid))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}
async fn write_group_conversation(gid: &str, msgs: &[DmMessage]) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(msgs).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(&group_conv_path(gid), &v).await
}

async fn touch_group(gid: &str, preview: &str, ts: i64, unread_delta: u32) {
    let mut groups = read_groups().await;
    if let Some(g) = groups.iter_mut().find(|g| g.id == gid) {
        g.last_ts = ts.max(g.last_ts);
        g.last_preview = preview.chars().take(120).collect();
        g.unread = g.unread.saturating_add(unread_delta);
        let _ = write_groups(&groups).await;
    }
}

/// The roster context embedded in every group message so receivers can
/// materialise / refresh the group with no separate invite step.
///
/// F-ROSTER-KEYPOISON: overlay MY OWN roster entry with a freshly SELF-SIGNED one
/// (`my_roster_member`) so the entry I fan out always carries a valid PoP over my
/// current keys — even in a group I didn't create (where my stored entry came from
/// the owner and has no PoP). Recipients can then pin my keys as a sealing key
/// instead of discovery-only, so MY delivery never regresses. Other members'
/// entries are forwarded verbatim (their own self-broadcasts carry their PoPs).
async fn group_ctx(g: &Group) -> Value {
    let mut members = g.members.clone();
    if let Ok(me) = ensure_profile().await {
        let mine = my_roster_member(&me.did_key, &me.name).await;
        match members.iter_mut().find(|m| m.did == me.did_key) {
            Some(slot) => *slot = mine,
            None => members.push(mine),
        }
    }
    json!({
        "id": g.id,
        "name": g.name,
        "members": members,
        "createdBy": g.created_by,
        "bio": g.bio,
        "avatarCid": g.avatar_cid,
        // governance — verified + applied by upsert_group_from_ctx on receive
        "epoch": g.epoch,
        "admins": g.admins,
        "blocked": g.blocked,
        "muted": g.muted,
        "grants": g.grants,
        "removed": g.removed,
    })
}

/// SLIM roster context — the SAME shape as `group_ctx` but each member carries
/// ONLY `did` + `name` (NO `peerPubkeys` / `peerTicket`). The PQ member keys
/// (~1.2 KB ML-KEM each) dominate the ctx size and push it past the ~4 KB gossip
/// cap → costly frag.rs fragmentation on EVERY group message. The keys only need
/// to travel occasionally (membership changes re-announce full via
/// add_group_members; the periodic full self-heals anyone who missed one), so the
/// steady-state group message rides this slim ctx instead.
///
/// SAFETY: a slim member entry must NEVER cause upsert_group_from_ctx to erase a
/// cached member's keys — see the "preserve cached keys" merge there.
fn slim_group_ctx(g: &Group) -> Value {
    let members: Vec<Value> = g
        .members
        .iter()
        .map(|m| json!({ "did": m.did, "name": m.name }))
        .collect();
    json!({
        "id": g.id,
        "name": g.name,
        "members": members,
        "createdBy": g.created_by,
        "bio": g.bio,
        "avatarCid": g.avatar_cid,
        // governance — same fields as group_ctx so a slim ctx still applies roles.
        "epoch": g.epoch,
        "admins": g.admins,
        "blocked": g.blocked,
        "muted": g.muted,
        "grants": g.grants,
        "removed": g.removed,
    })
}

/// Whether `did` may run admin/member-management ops on `g`. The owner
/// (`created_by`) is IMPLICITLY admin and always returns true; any DID listed in
/// `g.admins` is an admin too. (Source of truth for the simple admin API; the
/// signed `grants` system, when present, is verified by `verified_admins_from_grants`.)
// Retained for the governance/grant logic + future admin-tier features; the
// LOCAL mutation ops are now owner-gated via is_group_owner_or_legacy (req 8).
#[allow(dead_code)]
fn is_group_admin(g: &Group, did: &str) -> bool {
    did == g.created_by || g.admins.iter().any(|a| a == did)
}

/// Owner-authority for LOCAL group mutations (add/remove member, add admin, set
/// picture). True for the OWNER of an owned group, or for ANY member of a legacy
/// ownerless group (created before the createdBy field) — so legacy groups keep
/// working while owned groups are owner-only. Restricting these ops to the owner
/// (rather than the broader admin set) means a non-owner's change can't silently
/// no-op (it would be reverted by the owner's authoritative next ctx anyway); the
/// caller gets a clear error instead. Owner flows are unaffected.
fn is_group_owner_or_legacy(g: &Group, did: &str) -> bool {
    g.created_by.is_empty() || g.created_by == did
}

/// Whether `did` is barred from the group (kicked-and-blocked, or carries a
/// removal tombstone). A barred DID is NEVER re-added to the roster, its inbound
/// messages are dropped, and we never fan out to it — even if it reappears in a
/// (stale or forged) larger roster. The kick/block barrier (F-07).
fn is_group_barred(g: &Group, did: &str) -> bool {
    g.blocked.iter().any(|b| b == did) || g.removed.iter().any(|r| r.did == did)
}

/// Canonical bytes a `RoleGrant` is signed over by the group OWNER. Excludes the
/// `sig` field itself. ANY change to a field changes the bytes, so a member that
/// re-broadcasts the roster ctx can neither forge a new grant nor mutate one.
fn role_grant_canonical(grant: &RoleGrant) -> String {
    format!(
        "hey-role-grant:v1:{}:{}:{}:{}:{}:{}",
        grant.gid, grant.subject, grant.role, grant.epoch, grant.issuer, grant.nonce
    )
}

/// Verify a single `RoleGrant`: it MUST be issued by the group owner
/// (`issuer == owner_did`), carry a valid owner Ed25519 signature over the
/// canonical bytes, name this group (`gid`), and NOT attempt to assign owner
/// (role 2 is always rejected — ownership is immutable). Returns true only when
/// the grant is genuinely owner-authorized (F-06). An empty owner (legacy
/// ownerless group) has no enforceable authority, so no grant is ever honored.
fn verify_role_grant(grant: &RoleGrant, gid: &str, owner_did: &str) -> bool {
    if owner_did.is_empty() || grant.issuer != owner_did || grant.gid != gid {
        return false;
    }
    if grant.role == 2 {
        return false; // ownership is immutable — never grant owner via a RoleGrant
    }
    let pk = match did_key_to_public_key(owner_did) {
        Ok(pk) => pk,
        Err(_) => return false,
    };
    verify(role_grant_canonical(grant).as_bytes(), &grant.sig, &pk)
}

/// Materialize the admin DID set from owner-signed `grants`, applied in epoch
/// order so a later demote (role 0) overrides an earlier promote (role 1). Only
/// grants that pass `verify_role_grant` are honored (F-06); the owner is implicit
/// and never listed. When no valid grants are present this returns None so the
/// caller keeps the plain `admins` list (back-compat with the simple admin API).
fn verified_admins_from_grants(grants: &[RoleGrant], gid: &str, owner_did: &str) -> Option<Vec<String>> {
    let mut valid: Vec<&RoleGrant> = grants
        .iter()
        .filter(|gr| verify_role_grant(gr, gid, owner_did))
        .collect();
    if valid.is_empty() {
        return None;
    }
    // Apply in epoch order (stable for equal epochs) so the latest role wins.
    valid.sort_by_key(|gr| gr.epoch);
    let mut admins: Vec<String> = Vec::new();
    for gr in valid {
        admins.retain(|a| a != &gr.subject);
        if gr.role == 1 && gr.subject != owner_did {
            admins.push(gr.subject.clone());
        }
    }
    Some(admins)
}

const DECLINED_GROUPS_FILE: &str = "dm/declined-groups.json";

async fn read_declined_groups() -> Vec<String> {
    storage::read_json(DECLINED_GROUPS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Bootstrap a v2 Regular DM contact from KNOWN pubkeys + ticket (no invite
/// handshake), so messaging works immediately. Used by hey-social's follow
/// (`verified = true`: keys are self-asserted in the signed friend-link /
/// follow.request) AND by the group roster fan-out (`verified = false`: keys are
/// vouched by the group creator — pinned but UNVERIFIED).
///
/// KEY-CONTINUITY PINNING (the security fix): once a contact has pinned keys we
/// never silently replace them with DIFFERENT keys. A self-assertion
/// (`verified`) UPGRADES a prior unverified (roster) pin; any other mismatch is
/// REFUSED and logged (possible MITM/rotation — surface, don't auto-trust).
/// Returns true if a contact was created or upgraded.
/// Re-join (and UN-TOMBSTONE) every gossip queue a contact receives on. A prior
/// `delete_conversation` called `leave_topic`, which tombstones those topics so the
/// auto re-join (`ensure_topic`) skips them — without this, a re-paired chat would
/// never re-mesh ("re-scan to chat again doesn't work"). `peer::join_topic` maps to
/// the carrier `gossip_join` op, the ONLY path that lifts the leave-tombstone. Mirrors
/// the exact topic set `delete_conversation` tore down. Idempotent — safe on any re-add.
pub async fn rejoin_contact_topics(did: &str) {
    let me = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    let Some(c) = list_contacts().await.into_iter().find(|c| c.did == did) else {
        return;
    };
    let mut topics: Vec<String> = Vec::new();
    if let Some(q) = c.my_inbound_queue.as_deref() {
        topics.push(format!("{TOPIC_PREFIX_V2}/{q}"));
    }
    if matches!(c.mode, IdentityMode::Regular) && !me.is_empty() {
        topics.push(format!("{TOPIC_PREFIX_V2}/{}", pair_inbound_queue(&me, did)));
    }
    if let Some(q) = c.salted_queue.as_deref() {
        topics.push(format!("{TOPIC_PREFIX_V2}/{q}"));
    }
    for t in topics {
        let _ = peer::join_topic(&t).await; // gossip_join → clears the leave-tombstone + subscribes
    }
}

/// Idempotent per-contact self-repair: back-fill MISSING per-pair queues/pseudonyms so an
/// Active+keyed contact is is_v2_active() again and re-join topics.
/// NEVER touches peer_pubkeys/key_verified/key_changed/peer_salted/salted_self_ready_at.
///
/// `lift_hidden`: when true, also lift the soft-delete tombstone (un-hide the chat).
/// ONLY pass true on genuine user re-engagement (re-accept invite / re-follow) — the
/// boot sweep passes false so a soft-deleted chat is NOT resurrected on every relaunch.
pub async fn repair_contact(did: &str, lift_hidden: bool) {
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if did.is_empty() || did == my_did {
        return;
    }
    {
        let _g = contacts_gate().lock().await;
        let mut list = list_contacts().await;
        let Some(c) = list.iter_mut().find(|c| c.did == did) else {
            return;
        };
        if c.peer_pubkeys.is_none() {
            return;
        } // keyless: not our job
        let mut dirty = false;
        if c.status != ContactStatus::Active {
            c.status = ContactStatus::Active;
            dirty = true;
        }
        if c.my_inbound_queue.is_none() {
            c.my_inbound_queue = Some(random_hex(32));
            dirty = true;
        }
        if c.my_recv_pseudonym.is_none() {
            c.my_recv_pseudonym = Some(random_hex(16));
            dirty = true;
        }
        if c.my_send_pseudonym.is_none() {
            c.my_send_pseudonym = Some(random_hex(16));
            dirty = true;
        }
        if matches!(c.mode, IdentityMode::Regular) && c.their_inbound_queue.is_none() {
            c.their_inbound_queue = Some(pair_inbound_queue(did, &my_did));
            dirty = true;
        }
        // Heal a FALSE verify-gate ONLY for an already-verified, unchanged contact (recovers the
        // delete-chat-zeroed-last_ts regression) — never un-gate a genuinely unverified/changed key.
        if c.needs_verify_before_send && c.key_verified && !c.key_changed {
            c.needs_verify_before_send = false;
            dirty = true;
        }
        if dirty {
            let _ = write_contacts(&list).await;
        }
    }
    if lift_hidden {
        unhide_chat(did).await;
    }
    let _ = ensure_salted_queue(did).await;
    rejoin_contact_topics(did).await;
}

/// Startup sweep — repair every contact once per boot. Never un-hides a
/// soft-deleted chat (lift_hidden=false), so a deleted conversation stays gone.
pub async fn repair_all_contacts() {
    for c in list_contacts().await {
        repair_contact(&c.did, false).await;
    }
}

pub async fn bootstrap_contact_from_keys(
    did: &str,
    name: &str,
    keys: PeerKeys,
    ticket: Option<String>,
    verified: bool,
) -> bool {
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if did.is_empty() || did == my_did {
        return false;
    }
    let det = pair_inbound_queue(did, &my_did);
    // Serialize this contact-pin read-modify-write against the other receive-path
    // RMW (handshake/welcome/queue-rotation/touch) — closes the lost-update race.
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        match c.peer_pubkeys.clone() {
            Some(existing)
                if existing.x25519_pub_b64 == keys.x25519_pub_b64
                    && existing.ml_kem_pub_b64 == keys.ml_kem_pub_b64 =>
            {
                // Same keys already pinned. A self-assertion upgrades trust.
                if verified && !c.key_verified {
                    c.key_verified = true;
                    // The self-assertion re-confirms these keys — clear any prior
                    // safety-number alarm so it isn't left stuck on after re-verify.
                    c.key_changed = false;
                    if c.peer_ticket.is_none() {
                        c.peer_ticket = ticket;
                        // F-OWNER-TICKET-PoP: verified self-assertion ⇒ trusted dial anchor.
                        c.ticket_self_asserted = true;
                    }
                    let _ = write_contacts(&list).await;
                    return true;
                }
                return false;
            }
            Some(_) => {
                // Keys DIFFER from the pin. A self-assertion overrides a prior
                // UNVERIFIED (roster) pin; otherwise refuse + flag.
                if verified && !c.key_verified {
                    c.peer_pubkeys = Some(keys);
                    c.key_verified = true;
                    // Adopting the self-asserted keys resolves the pin — clear any
                    // prior safety-number alarm (it referred to the old, now-replaced
                    // pin), so the alarm stays MEANINGFUL (set only on a real,
                    // un-adopted change).
                    c.key_changed = false;
                    // F-11: the salted topic is derived from the peer's X25519
                    // key — invalidate the cache so it re-derives from the new key.
                    c.salted_queue = None;
                    if ticket.is_some() {
                        c.peer_ticket = ticket;
                        // F-OWNER-TICKET-PoP: verified self-assertion ⇒ trusted dial anchor.
                        c.ticket_self_asserted = true;
                    }
                    let _ = write_contacts(&list).await;
                    return true;
                }
                // SAFETY-NUMBER ALARM: the incoming keys differ from the pin AND
                // we did NOT adopt them (the contact was already verified, so the
                // upgrade branch above didn't fire). Keep refusing to replace the
                // pin — but if this contact had been VERIFIED, raise the
                // safety-number-changed flag so the UI warns the user. (For an
                // unverified/roster contact a mismatch isn't alarming the same
                // way; leave that path unchanged.)
                if c.key_verified && !c.key_changed {
                    c.key_changed = true;
                    c.key_verified = false;
                    // Re-raise the first-send gate alongside the alarm so the invariant
                    // "key_changed ⟹ needs_verify_before_send" holds at the SOURCE: every
                    // needs_verify_before_send consumer (the text gate + wallet-card/call/feed-key
                    // self-checks) then covers this key-change without each having to special-case
                    // key_changed. (The consumers also check key_changed directly = defense in depth.)
                    c.needs_verify_before_send = true;
                    let _ = write_contacts(&list).await;
                }
                crate::plat::warn(&format!(
                    "[hey-core] key mismatch for {} — refusing to replace pinned keys (possible MITM or key rotation)",
                    did
                ));
                return false;
            }
            None => {
                // Legacy/keyless contact — adopt these keys + make it v2-active.
                c.peer_pubkeys = Some(keys);
                c.key_verified = verified;
                c.mode = IdentityMode::Regular;
                c.status = ContactStatus::Active;
                if c.my_inbound_queue.is_none() {
                    c.my_inbound_queue = Some(random_hex(32));
                }
                if c.my_recv_pseudonym.is_none() {
                    c.my_recv_pseudonym = Some(random_hex(16));
                }
                if c.their_inbound_queue.is_none() {
                    c.their_inbound_queue = Some(det);
                }
                if c.my_send_pseudonym.is_none() {
                    c.my_send_pseudonym = Some(random_hex(16));
                }
                if ticket.is_some() {
                    c.peer_ticket = ticket;
                    // F-OWNER-TICKET-PoP: self-asserted only on a VERIFIED bootstrap
                    // (owner-roster bootstrap passes verified=false ⇒ stays unasserted).
                    c.ticket_self_asserted = verified;
                }
                // F-ADDR-CARD-UNVERIFIED (keyless-adopt): a discovery-only contact is created with
                // needs_verify_before_send=FALSE (it had no keys, so nothing could seal to it). The
                // moment we adopt keys from an UNVERIFIED source (an unsolicited follow bootstraps
                // with verified=false) the wallet address card / call ticket COULD seal to those
                // attacker-substituted keys — so gate it, exactly like the create path (4610).
                // GRANDFATHER (mirrors mark_needs_verify_before_send): a verified self-assertion
                // clears the gate; a FRESH unverified adopt (not yet verified AND no outbound
                // history) raises it; an ESTABLISHED keyless chat (last_ts != 0) is left untouched
                // so adopting keys never blocks an ongoing conversation.
                if verified {
                    c.needs_verify_before_send = false;
                } else if !c.key_verified && c.last_ts == 0 {
                    c.needs_verify_before_send = true;
                }
                let _ = write_contacts(&list).await;
                return true;
            }
        }
    }
    // No existing — create a new Regular contact (deterministic pair queue;
    // minted placeholder queues let is_v2_active() pass).
    list.push(DmContact {
        did: did.to_string(),
        peer_ticket: ticket,
        // F-OWNER-TICKET-PoP: the ticket is self-asserted only when this bootstrap
        // is a VERIFIED self-assertion (signed follow/invite/key-confirm). The
        // group-roster bootstrap calls this with verified=false and an
        // OWNER-controlled ticket ⇒ NOT self-asserted (fails closed for the dial).
        ticket_self_asserted: verified,
        name: name.to_string(),
        last_ts: 0,
        last_preview: String::new(),
        unread: 0,
        my_inbound_queue: Some(random_hex(32)),
        my_recv_pseudonym: Some(random_hex(16)),
        their_inbound_queue: Some(det),
        my_send_pseudonym: Some(random_hex(16)),
        peer_pubkeys: Some(keys),
        key_pop: None,
        status: ContactStatus::Active,
        mode: IdentityMode::Regular,
        anon_identity: None,
        ratchet_capable: false,
        key_verified: verified,
        key_changed: false,
        oob_verified: false,
        my_queue_rotated_at: 0,
        my_queue_msg_count: 0,
        retired_queues: Vec::new(),
        salted_queue: None,
        peer_salted: false,
        peer_salted_at: 0,
        salted_self_ready_at: 0,
        // F-ADDR-CARD-UNVERIFIED: default to GATED whenever the keys aren't verified.
        // Relying on the caller to mark_needs_verify_before_send left re-creation paths
        // (start_chat -> bootstrap_dm, group-roster bootstrap) ungated, so the wallet
        // address card could be sealed to attacker-substituted keys. A verified contact
        // (signed link, verified=true) starts ungated as before; everyone else must be
        // cleared via verify_contact/confirm_unverified_send first.
        needs_verify_before_send: !verified,
    });
    let _ = write_contacts(&list).await;
    true
}

/// F-FOLLOW-PoP: flag (or clear) the "confirm before first send" gate for a
/// contact. The caller passes `flag=true` ONLY when this contact's keys were
/// pinned from an UNVERIFIED, UNSIGNED source AND no message has been sealed to
/// them yet. NEVER raises the gate on a contact that is already verified or that
/// already has outbound history (grandfathered): such a contact is left
/// untouched so existing chats keep working. Returns the effective flag state.
pub async fn mark_needs_verify_before_send(did: &str, flag: bool) -> bool {
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let Some(c) = list.iter_mut().find(|c| c.did == did) else {
        return false;
    };
    if flag {
        // Don't gate an already-trusted contact, nor one we've already messaged.
        if c.key_verified || c.last_ts != 0 {
            return false;
        }
        if !c.needs_verify_before_send {
            c.needs_verify_before_send = true;
            let _ = write_contacts(&list).await;
        }
        true
    } else {
        if c.needs_verify_before_send {
            c.needs_verify_before_send = false;
            let _ = write_contacts(&list).await;
        }
        false
    }
}

/// F-FOLLOW-PoP: the user reviewed an unverified-from-unsigned-source contact and
/// chose to send anyway (without a full safety-number verification). Clears ONLY
/// the send gate — the keys remain pinned UNVERIFIED (key_verified stays false),
/// so the safety-number-changed alarm still fires later if they ever rotate.
pub async fn confirm_unverified_send(did: &str) -> Result<(), String> {
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let Some(c) = list.iter_mut().find(|c| c.did == did) else {
        return Err("no such contact".into());
    };
    // SECURITY (downgrade-merge MITM): a casual "send anyway" is NOT sufficient consent for a
    // contact whose previously-VERIFIED keys CHANGED — that is exactly the key-substitution case
    // the safety-number alarm exists for. Refuse to clear the gate here; ONLY an explicit
    // out-of-band safety-number re-verification (verify_contact, which clears key_changed) may
    // re-open user sends + the wallet-card/call-ticket auto-shares to the new keys. Without this,
    // the UI's auto-confirm-on-first-send would silently tear down the F-DUPMERGE-GATE.
    if c.key_changed {
        return Err("key changed — verify the safety number before sending".into());
    }
    if c.needs_verify_before_send {
        c.needs_verify_before_send = false;
        write_contacts(&list).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// SAFETY-NUMBER VERIFICATION: the user compared this contact's safety number
/// out-of-band (or re-verified after a key-change alarm). Mark the pinned keys
/// VERIFIED and clear any key_changed alarm. Persists. No-op error if unknown.
pub async fn verify_contact(did: &str) -> Result<(), String> {
    // Serialize this contact RMW against the receive-path bootstrap/handshake RMW — without
    // the gate, a concurrent re-bootstrap (record_follower/process_sealed_follows fires every
    // ~2s on the scanned side) reads the old list and writes it back, CLOBBERING the verify so
    // key_verified flips back to false and the "unverified/verify" badge never clears.
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let Some(c) = list.iter_mut().find(|c| c.did == did) else {
        return Err("no such contact".into());
    };
    c.key_verified = true;
    c.key_changed = false;
    // The ONLY place oob_verified is set: an explicit out-of-band safety-number comparison.
    c.oob_verified = true;
    // Verifying the safety number is strictly stronger than the F-FOLLOW-PoP
    // confirm gate — clear it so a verified contact never blocks on the first send.
    c.needs_verify_before_send = false;
    write_contacts(&list).await.map_err(|e| e.to_string())
}

/// F-ROSTER-KEYPOISON: canonical bytes a member self-signs to PROVE possession of
/// the PQ keys it advertises in a group roster. FIXED-key-order JSON over
/// `{did,k,x}` — TICKET-INDEPENDENT (a member's ticket rotates; their keys don't),
/// so the same PoP keeps verifying after a relay/network change. The signer
/// (`sign_member_pop`) and the verifier (`verify_member_pop`) build it identically
/// so the Ed25519 signature round-trips byte-for-byte. Mirrors `canonical_follow_msg`.
fn canonical_member_pop(did: &str, keys: &PeerKeys) -> Vec<u8> {
    json!({ "did": did, "k": keys.ml_kem_pub_b64, "x": keys.x25519_pub_b64 })
        .to_string()
        .into_bytes()
}

/// Sign my own roster proof-of-possession with the session identity (provider- or
/// seed-backed via `sign_bytes`). None when keys are missing or signing is
/// unavailable (the entry then carries no PoP and recipients pin it discovery-only,
/// exactly as a legacy entry would — never a regression).
async fn sign_member_pop(did: &str, keys: &PeerKeys) -> Option<String> {
    if keys.x25519_pub_b64.is_empty() || keys.ml_kem_pub_b64.is_empty() {
        return None;
    }
    let auth = session::current().map(|s| s.auth_key_hex).unwrap_or_default();
    sign_bytes(&canonical_member_pop(did, keys), &auth).await.ok()
}

/// Verify a roster member's proof-of-possession: the `key_pop` signature must
/// verify against the member's OWN did:key over `canonical_member_pop(did, keys)`.
/// Returns false when the PoP is absent, the keys are missing, the did:key can't
/// be parsed, or the signature doesn't check out — every "not provable" case maps
/// to false so the caller falls back to the safe discovery-only pin.
fn verify_member_pop(m: &GroupMember) -> bool {
    let (Some(keys), Some(sig)) = (m.peer_pubkeys.as_ref(), m.key_pop.as_ref()) else {
        return false;
    };
    let Ok(pk) = did_key_to_public_key(&m.did) else {
        return false;
    };
    verify(&canonical_member_pop(&m.did, keys), sig, &pk)
}

/// Roster-member variant — bootstraps from a GroupMember's carried keys+ticket.
/// `verified = false`: roster keys are vouched by the group creator (third
/// party), not self-asserted — pinned but unverified until the member directly
/// confirms (e.g. their own follow.request/invite upgrades it).
///
/// F-ROSTER-KEYPOISON: the owner builds the roster and could pin ATTACKER keys
/// under a co-member's DID (the victim's 1:1 DMs to that co-member would then seal
/// to the owner). Defence: only pin the carried keys AS A SEALING KEY when the
/// entry carries a VALID per-member proof-of-possession (`verify_member_pop` — a
/// self-signature the owner can't forge). When the PoP is ABSENT or INVALID we pin
/// DISCOVERY-ONLY: refresh an already-known contact's ticket so the pair-queue
/// still meshes, but for a brand-NEW contact create a keyless record (did+ticket,
/// NO `peer_pubkeys`) so a fresh DM can't seal to unproven keys. The member's own
/// later signed follow/invite/handshake (or a self-broadcast roster entry carrying
/// a valid PoP) upgrades it to a real pinned key via `bootstrap_contact_from_keys`.
async fn bootstrap_roster_contact(m: &GroupMember, _my_did: &str, allow_new: bool) {
    let Some(keys) = m.peer_pubkeys.clone() else {
        return;
    };
    let known = list_contacts().await.iter().any(|c| c.did == m.did);
    // Legacy group with NO recorded owner: a non-owner member could forge the
    // roster, so only REFRESH already-known contacts — never pin a brand-new
    // (attacker-chosen) did+keys under it.
    if !allow_new && !known {
        return;
    }
    // PoP valid ⇒ the keys are provably this member's own ⇒ safe to pin as a
    // sealing key (still unverified — a direct self-assertion upgrades it later).
    if verify_member_pop(m) {
        let _ = bootstrap_contact_from_keys(&m.did, &m.name, keys, m.peer_ticket.clone(), false).await;
        // Persist the proven PoP so OUR re-broadcast of the roster forwards it (the
        // proof propagates member→member, not just from the member's own fan-out).
        record_contact_key_pop(&m.did, m.key_pop.as_deref()).await;
        return;
    }
    // PoP ABSENT or INVALID ⇒ discovery-only.
    if known {
        // Known contact: never replace/inject keys from an unproven roster entry —
        // only refresh the ticket so the existing pair-queue keeps meshing.
        if let Some(t) = m.peer_ticket.as_deref() {
            // OWNER-supplied roster ticket: refresh for pair-queue meshing only —
            // NOT a trusted group-call dial anchor (self_asserted=false).
            refresh_peer_ticket(&m.did, t, false).await;
        }
    } else {
        // Brand-new contact: pin DISCOVERY-ONLY (did + ticket, NO sealing keys) so a
        // fresh 1:1 DM can't seal to owner-forged keys. A later proven assertion
        // adds the real keys.
        bootstrap_discovery_only_contact(&m.did, &m.name, m.peer_ticket.clone()).await;
    }
}

/// F-ROSTER-KEYPOISON discovery-only pin: record `did` (+optional ticket) as a
/// KEYLESS contact so it's discoverable and its pair-queue can mesh, but WITHOUT
/// `peer_pubkeys` — so nothing seals a message to keys we can't prove belong to
/// this member. No-op if the contact already exists (never downgrades a real pin).
async fn bootstrap_discovery_only_contact(did: &str, name: &str, ticket: Option<String>) {
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if did.is_empty() || did == my_did {
        return;
    }
    let det = pair_inbound_queue(did, &my_did);
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    if list.iter().any(|c| c.did == did) {
        return; // already known — leave its (possibly real) keys untouched
    }
    list.push(DmContact {
        did: did.to_string(),
        peer_ticket: ticket,
        ticket_self_asserted: false, // owner/roster-bootstrapped — fail closed until self-asserted
        name: name.to_string(),
        last_ts: 0,
        last_preview: String::new(),
        unread: 0,
        my_inbound_queue: Some(random_hex(32)),
        my_recv_pseudonym: Some(random_hex(16)),
        their_inbound_queue: Some(det),
        my_send_pseudonym: Some(random_hex(16)),
        peer_pubkeys: None, // DISCOVERY-ONLY — no sealing key until proven
        key_pop: None,
        status: ContactStatus::Active,
        mode: IdentityMode::Regular,
        anon_identity: None,
        ratchet_capable: false,
        key_verified: false,
        key_changed: false,
        oob_verified: false,
        my_queue_rotated_at: 0,
        my_queue_msg_count: 0,
        retired_queues: Vec::new(),
        salted_queue: None,
        peer_salted: false,
        peer_salted_at: 0,
        salted_self_ready_at: 0,
        needs_verify_before_send: false,
    });
    let _ = write_contacts(&list).await;
}

/// Persist a contact's PROVEN proof-of-possession so we forward it when WE build a
/// roster that includes them. The caller has already verified this PoP against the
/// entry's keys; a None/empty pop is ignored, and a stale pop is self-correcting
/// (if our pinned keys later differ, a recipient's `verify_member_pop` simply fails
/// and falls back to the safe discovery-only pin). Writes only on change.
async fn record_contact_key_pop(did: &str, pop: Option<&str>) {
    let Some(pop) = pop.filter(|p| !p.is_empty()) else {
        return;
    };
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        if c.key_pop.as_deref() != Some(pop) {
            c.key_pop = Some(pop.to_string());
            let _ = write_contacts(&list).await;
        }
    }
}

/// Create / refresh a local group record from a received `group` context. Adds
/// the group if new (as PENDING — join-consent), refreshes name/roster/meta if
/// it grew, bootstraps pairwise channels to unknown members, and honours a prior
/// decline. Returns the group id (None if declined / no id).
async fn upsert_group_from_ctx(ctx: &Value, sender_did: &str) -> Option<String> {
    let id = ctx.get("id").and_then(|v| v.as_str())?.to_string();
    if id.is_empty() {
        return None;
    }
    // Honour a prior decline — ignore all future ctx for a declined group.
    if read_declined_groups().await.iter().any(|d| d == &id) {
        return None;
    }
    let name = ctx
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Group")
        .to_string();
    // ROSTER CAP (anti-DoS): REJECT — never truncate — an oversized roster BEFORE
    // we deserialize / merge / bootstrap pairwise channels. A forged huge roster
    // would otherwise force thousands of pairwise key bootstraps. Checked on the
    // RAW array length so we don't even deserialize a hostile payload. MAX is far
    // above any real group, so legitimate groups are unaffected.
    if let Some(arr) = ctx.get("members").and_then(|v| v.as_array()) {
        if arr.len() > MAX_GROUP_MEMBERS {
            crate::plat::warn(&format!(
                "[hey-core] rejecting group {} ctx: roster {} > cap {}",
                id,
                arr.len(),
                MAX_GROUP_MEMBERS
            ));
            return None;
        }
    }
    // GOVERNANCE-ARRAY CAP (anti-DoS): the same raw-array length rejection for the
    // blocked / muted / removed / grants governance arrays. Checked on the RAW array
    // BEFORE deserialize/merge/store so a forged huge governance array can't be
    // adopted at any of the downstream sites (ctx-extract, ownerless append, new-group
    // store all read these same arrays). MAX_GROUP_MEMBERS bounds them too — far above
    // any real group's governance state, so legitimate groups are unaffected.
    for field in ["blocked", "muted", "removed", "grants"] {
        if let Some(arr) = ctx.get(field).and_then(|v| v.as_array()) {
            if arr.len() > MAX_GROUP_MEMBERS {
                crate::plat::warn(&format!(
                    "[hey-core] rejecting group {} ctx: {} array {} > cap {}",
                    id,
                    field,
                    arr.len(),
                    MAX_GROUP_MEMBERS
                ));
                return None;
            }
        }
    }
    let members: Vec<GroupMember> = ctx
        .get("members")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let created_by = ctx.get("createdBy").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let bio = ctx.get("bio").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let avatar_cid = ctx.get("avatarCid").and_then(|v| v.as_str()).map(|s| s.to_string());
    // Admin roster carried in the ctx — adopt the owner's list so every member
    // learns who the admins (and the group picture) are.
    let admins: Vec<String> = ctx
        .get("admins")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // Governance fields (F-07): a missing epoch is 0 (legacy peers serialize 0),
    // so existing groups still at epoch 0 keep messaging. blocked/removed are HARD
    // barriers; muted is soft; grants are owner-signed (verified below).
    let ctx_epoch = ctx.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
    let ctx_blocked: Vec<String> = ctx
        .get("blocked")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let ctx_muted: Vec<String> = ctx
        .get("muted")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let ctx_removed: Vec<RemovedMember> = ctx
        .get("removed")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let ctx_grants: Vec<RoleGrant> = ctx
        .get("grants")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let my_did = ensure_profile().await.ok().map(|m| m.did_key).unwrap_or_default();

    let mut groups = read_groups().await;
    // SB-3: roster, ownership, owner-metadata AND key-bootstrap are accepted only
    // from the recorded owner (the cryptographically-verified inner sender_did
    // must equal created_by), or when no owner is recorded yet (legacy / first
    // sighting → TOFU, gated by the join-consent prompt). A non-owner member
    // routinely re-announces the full group ctx in every group message, so without
    // this gate ANY member could rewrite the membership, seize ownership, or inject
    // attacker-controlled member keys for pinning. The owner broadcasts changes
    // directly to all members, so legitimate propagation is unaffected.
    let owner_authoritative = match groups.iter().find(|g| g.id == id) {
        Some(g) => g.created_by.is_empty() || g.created_by == sender_did,
        None => true,
    };

    // Anti-rollback (F-07): NEVER let an older-epoch ctx overwrite governance/roster
    // state. A ctx with epoch < the one already applied is stale; we reject the
    // governance/roster apply (but still let name + key-bootstrap below proceed for a
    // SAME-epoch ctx). Missing epoch deserializes 0, so a legacy 0-epoch peer can
    // still update a group that's still at epoch 0 (back-compat). The first time a
    // higher epoch is seen, this group leaves the legacy 0-epoch world for good.
    let stored_epoch = groups.iter().find(|g| g.id == id).map(|g| g.epoch).unwrap_or(0);
    let epoch_ok = ctx_epoch >= stored_epoch;

    // OWNERLESS-group hole (F-07): a brand-new inbound group ctx that names NO owner
    // (empty createdBy) has no enforceable authority — anyone could later forge a
    // shrink/grow with no anti-rollback root. Reject materialising such a group.
    // (Existing ownerless legacy groups already held locally keep working — this
    // only blocks NEW ownerless ones.)
    if created_by.is_empty() && !groups.iter().any(|g| g.id == id) {
        return None;
    }

    // Bootstrap pairwise channels ONLY from an authoritative roster AND only once
    // the user has CONSENTED (the group is not pending) — so a forged/unsolicited
    // group can't get attacker DIDs+keys pinned before you accept it. `allow_new`
    // gates pinning UNKNOWN dids to owner-vouched (created_by) rosters; a legacy
    // ownerless group may only refresh already-known contacts.
    let already_accepted = groups.iter().find(|g| g.id == id).map(|g| !g.pending).unwrap_or(false);
    // The effective bar set for this apply = the stored barriers UNION the (newer-or-
    // equal-epoch) ctx barriers. Used to keep a barred DID from being bootstrapped or
    // re-added even via a larger roster (F-07).
    let mut bar_set: Vec<String> = groups
        .iter()
        .find(|g| g.id == id)
        .map(|g| {
            let mut s: Vec<String> = g.blocked.clone();
            s.extend(g.removed.iter().map(|r| r.did.clone()));
            s
        })
        .unwrap_or_default();
    if owner_authoritative && epoch_ok {
        bar_set.extend(ctx_blocked.iter().cloned());
        bar_set.extend(ctx_removed.iter().map(|r| r.did.clone()));
    }
    let is_barred = |did: &str| bar_set.iter().any(|b| b == did);
    if owner_authoritative && already_accepted {
        let allow_new = groups
            .iter()
            .find(|g| g.id == id)
            .map(|g| !g.created_by.is_empty())
            .unwrap_or(!created_by.is_empty());
        for m in &members {
            if is_barred(&m.did) {
                continue; // never bootstrap a kicked/blocked DID
            }
            bootstrap_roster_contact(m, &my_did, allow_new).await;
        }
    }

    if let Some(g) = groups.iter_mut().find(|g| g.id == id) {
        // EPOCH-DRIVEN governance + roster apply (F-07). Only an authoritative owner
        // ctx that is not a rollback (epoch >= stored) may change governed state. A
        // newer epoch MAY SHRINK the roster (kick) — we no longer require length
        // growth. Barred DIDs are filtered out as a hard barrier on every apply.
        if owner_authoritative && epoch_ok {
            // The group NAME is owner-set metadata — apply it INSIDE the gate so a
            // non-owner (or a rollback) ctx can't rename an OWNED group. A genuinely
            // ownerless legacy group stays permissive (owner_authoritative is true
            // for any sender when created_by is empty), so its name still refreshes.
            if !name.is_empty() {
                g.name = name;
            }
            // Merge the barrier sets FIRST so the roster filter below is complete.
            for did in &ctx_blocked {
                if !g.blocked.iter().any(|b| b == did) {
                    g.blocked.push(did.clone());
                }
            }
            for rm in &ctx_removed {
                match g.removed.iter_mut().find(|r| r.did == rm.did) {
                    Some(r) => r.epoch = r.epoch.max(rm.epoch),
                    None => g.removed.push(rm.clone()),
                }
            }
            // Roster apply. A newer epoch may SHRINK the roster; a same-epoch ctx
            // takes the larger roster (avoids churn from out-of-order slim/full
            // announces). KEY-PRESERVING MERGE: a SLIM ctx (members with NO
            // peerPubkeys/peerTicket) must NEVER erase a cached member's keys — only
            // a FULL ctx member may set/refresh keys; a slim entry keeps whatever we
            // already have. Barred DIDs are NEVER re-added.
            if ctx_epoch > g.epoch || members.len() > g.members.len() {
                let incoming: Vec<GroupMember> =
                    members.into_iter().filter(|m| !is_barred(&m.did)).collect();
                let prev = std::mem::take(&mut g.members);
                let mut merged: Vec<GroupMember> = Vec::with_capacity(incoming.len());
                for mut m in incoming {
                    if m.peer_pubkeys.is_none() {
                        if let Some(local) = prev.iter().find(|p| p.did == m.did) {
                            // Slim entry — preserve the cached keys + ticket.
                            m.peer_pubkeys = local.peer_pubkeys.clone();
                            if m.peer_ticket.is_none() {
                                m.peer_ticket = local.peer_ticket.clone();
                            }
                        }
                    } else if m.peer_ticket.is_none() {
                        // Full keys but no ticket carried — keep a cached ticket.
                        if let Some(local) = prev.iter().find(|p| p.did == m.did) {
                            m.peer_ticket = local.peer_ticket.clone();
                        }
                    }
                    merged.push(m);
                }
                g.members = merged;
            }
            g.members.retain(|m| !is_barred(&m.did));
            // Owner-set metadata propagated via the creator's ctx. NEVER transition
            // an OWNED group's created_by set->empty (an empty ctx createdBy is
            // ignored here), so ownership can't be stripped from an owned group.
            if !created_by.is_empty() {
                g.created_by = created_by.clone();
            }
            if !bio.is_empty() {
                g.bio = bio;
            }
            if avatar_cid.is_some() {
                g.avatar_cid = avatar_cid;
            }
            // muted is soft moderation — adopt the owner's list wholesale.
            if !ctx_muted.is_empty() {
                g.muted = ctx_muted;
            }
            // Admins: prefer the owner-SIGNED grants when present (F-06: each grant's
            // sig is verified against the owner DID), else fall back to the plain
            // admin list for back-compat with the simple admin API.
            if !ctx_grants.is_empty() {
                g.grants = ctx_grants
                    .iter()
                    .filter(|gr| verify_role_grant(gr, &id, &g.created_by))
                    .cloned()
                    .collect();
            }
            if let Some(derived) = verified_admins_from_grants(&g.grants, &id, &g.created_by) {
                g.admins = derived;
            } else if !admins.is_empty() {
                g.admins = admins;
            }
            // Advance the epoch LAST so subsequent rollbacks are rejected.
            g.epoch = g.epoch.max(ctx_epoch);
        }
    } else {
        // GROUP COUNT CAP (anti-DoS): refuse to materialise a NEW (previously
        // unknown) group once we already hold MAX_GROUPS. Existing groups always
        // continue to update (handled in the branch above); this only blocks an
        // unbounded flood of forged new groups. Far above any real membership.
        if groups.len() >= MAX_GROUPS {
            crate::plat::warn(&format!(
                "[hey-core] rejecting new group {} ctx: at group cap {}",
                id, MAX_GROUPS
            ));
            return None;
        }
        // NEW received group → PENDING (join-consent). The UI shows accept /
        // decline; until then it's a marked-pending row. Old (consent-unaware)
        // UI ignores the flag and shows it as a normal group — no regression.
        // Barred DIDs carried in the very first ctx are filtered out immediately.
        let init_members: Vec<GroupMember> =
            members.into_iter().filter(|m| !is_barred(&m.did)).collect();
        // Only honor owner-signed grants for the materialized admin list.
        let init_grants: Vec<RoleGrant> = ctx_grants
            .iter()
            .filter(|gr| verify_role_grant(gr, &id, &created_by))
            .cloned()
            .collect();
        let init_admins =
            verified_admins_from_grants(&init_grants, &id, &created_by).unwrap_or(admins);
        groups.push(Group {
            id: id.clone(),
            name,
            members: init_members,
            last_ts: now_ms(),
            last_preview: String::new(),
            unread: 0,
            created_by,
            bio,
            avatar_cid,
            admins: init_admins,
            pending: true,
            epoch: ctx_epoch,
            blocked: ctx_blocked,
            muted: ctx_muted,
            grants: init_grants,
            removed: ctx_removed,
            ..Default::default()
        });
    }
    let _ = write_groups(&groups).await;
    Some(id)
}

/// Accept a pending (join-consent) group — flip it to active.
pub async fn accept_group(group_id: &str) -> Result<(), String> {
    let mut groups = read_groups().await;
    // Flip to active AND capture the roster to bootstrap — key-pinning is deferred
    // to HERE (explicit user consent) instead of on first sighting.
    let (members, allow_new) = match groups.iter_mut().find(|g| g.id == group_id) {
        Some(g) => {
            g.pending = false;
            (g.members.clone(), !g.created_by.is_empty())
        }
        None => return Err("no such group".into()),
    };
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    let my_did = ensure_profile().await.ok().map(|m| m.did_key).unwrap_or_default();
    for m in &members {
        bootstrap_roster_contact(m, &my_did, allow_new).await;
    }
    Ok(())
}

/// Decline a pending group — drop it and remember the decline so future ctx for
/// the same id is ignored (no re-materialise).
pub async fn decline_group(group_id: &str) -> Result<(), String> {
    let mut groups = read_groups().await;
    groups.retain(|g| g.id != group_id);
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    let mut declined = read_declined_groups().await;
    if !declined.iter().any(|d| d == group_id) {
        declined.push(group_id.to_string());
        if let Ok(v) = serde_json::to_value(&declined) {
            let _ = storage::write_json(DECLINED_GROUPS_FILE, &v).await;
        }
    }
    Ok(())
}

/// Owner-only: set a group's bio + avatar (CID of a pre-uploaded image) and
/// re-announce so members see it. Rejects non-owners (legacy groups with no
/// recorded owner are editable by anyone for back-compat).
pub async fn set_group_meta(
    group_id: &str,
    bio: &str,
    avatar_cid: Option<String>,
) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let mut groups = read_groups().await;
    let g = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    if !g.created_by.is_empty() && g.created_by != me.did_key {
        return Err("only the group owner can edit group info".into());
    }
    g.bio = bio.chars().take(280).collect();
    if avatar_cid.is_some() {
        g.avatar_cid = avatar_cid;
    }
    g.epoch = g.epoch.saturating_add(1);
    let group = g.clone();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    let ctx = group_ctx(&group).await;
    for m in &group.members {
        if m.did == me.did_key || is_group_barred(&group, &m.did) {
            continue;
        }
        let _ = send_body_to_contact(&m.did, &json!({ "group": ctx })).await;
    }
    // Push the freshly-enqueued meta announce NOW instead of waiting for the next
    // poll cycle — otherwise an info change (bio/avatar) only reaches members when
    // something else flushes the outbox (e.g. the next group message). Mirrors the
    // immediate flush after a group-message fan-out.
    crate::api::outbox::flush().await;
    Ok(())
}

/// Roster entry for a contact — carries their pubkeys + ticket so OTHER
/// recipients can bootstrap a pairwise channel to them (the 3+ fan-out fix).
/// F-ROSTER-KEYPOISON: forward the contact's captured self-signed proof-of-
/// possession (`key_pop`) so recipients can verify these keys are genuinely the
/// member's own before pinning them as a sealing key. None ⇒ no PoP captured ⇒
/// recipients pin discovery-only.
fn roster_member(c: &DmContact) -> GroupMember {
    GroupMember {
        did: c.did.clone(),
        name: if c.name.trim().is_empty() {
            short_did_label(&c.did)
        } else {
            c.name.clone()
        },
        peer_pubkeys: c.peer_pubkeys.clone(),
        peer_ticket: c.peer_ticket.clone(),
        key_pop: c.key_pop.clone(),
    }
}

/// My own roster entry — my pubkeys + ticket so members who don't already have
/// me as a contact can bootstrap a channel to me. F-ROSTER-KEYPOISON: SELF-SIGN a
/// proof-of-possession over my own (did, keys) so OTHER members can verify my keys
/// are genuinely mine and pin them as a sealing key (no owner could forge this).
async fn my_roster_member(me_did: &str, me_name: &str) -> GroupMember {
    let keys = my_pubkeys().await;
    let key_pop = match keys.as_ref() {
        Some(k) => sign_member_pop(me_did, k).await,
        None => None,
    };
    GroupMember {
        did: me_did.to_string(),
        name: me_name.to_string(),
        peer_pubkeys: keys,
        // F-FOLLOWANNOUNCE-TICKET-LEAK (sibling): the roster fans my ticket out to
        // EVERY group member, so IP-cap it the same way the DM `nt` stamp is —
        // relays + a few same-LAN hints, never my full direct-IP set. Recipients
        // bootstrap via connect() which only needs the EndpointId + relays.
        peer_ticket: peer::my_ticket().await.map(|t| compact_nt_ticket(&t)),
        key_pop,
    }
}

/// Create a group from EXISTING active contacts and announce it to them.
/// `member_dids` are the OTHER members (self is added automatically). The roster
/// carries each member's pubkeys + ticket so recipients can bootstrap pairwise
/// channels to members they don't already know (3+ fan-out).
pub async fn create_group(name: &str, member_dids: Vec<String>) -> Result<String, String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let contacts = list_contacts().await;
    let mut members = vec![my_roster_member(&me.did_key, &me.name).await];
    for did in &member_dids {
        if *did == me.did_key {
            continue;
        }
        let c = contacts
            .iter()
            .find(|c| c.did == *did && c.is_v2_active())
            .ok_or_else(|| {
                format!(
                    "{} is not an active contact — add them first",
                    short_did_label(did)
                )
            })?;
        if members.iter().any(|m| m.did == *did) {
            continue;
        }
        members.push(roster_member(c));
    }
    if members.len() < 2 {
        return Err("a group needs at least one other member".into());
    }
    let group = Group {
        id: random_hex(16),
        name: name.trim().to_string(),
        members,
        last_ts: now_ms(),
        last_preview: "Group created".into(),
        unread: 0,
        created_by: me.did_key.clone(),
        bio: String::new(),
        avatar_cid: None,
        pending: false,
        ..Default::default()
    };
    let mut groups = read_groups().await;
    groups.push(group.clone());
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    // Announce: fan out a roster-only message so each member materialises the
    // group. Best-effort — a member offline now gets it on the first text.
    // FAN-OUT: join_all the per-member sends. NOTE: on NATIVE hey-core is
    // fake-async on a current_thread executor, so these sends do NOT actually
    // overlap — each send_body_to_contact's join_topic_with neighbor wait runs
    // STRUCTURALLY SERIALLY. join_all is kept because it's harmless on native
    // and a real overlap on wasm; the latency win on native comes from the
    // cheap cold-topic gate (peer_receiver) + faster poll/self-heal + the
    // immediate outbox flush below, NOT from join_all overlap. Errors ignored.
    let ctx = group_ctx(&group).await;
    let body = json!({ "group": ctx });
    let sends = group
        .members
        .iter()
        .filter(|m| m.did != me.did_key && !is_group_barred(&group, &m.did))
        .map(|m| send_body_to_contact(&m.did, &body));
    let _ = futures_util::future::join_all(sends).await;
    // Push the freshly-enqueued announce NOW instead of waiting for the next
    // poll cycle (faster group materialisation on the other devices).
    crate::api::outbox::flush().await;
    Ok(group.id)
}

/// Send a text message to every member of a group (pairwise fan-out).
pub async fn send_group_message(group_id: &str, text: &str) -> Result<DmMessage, String> {
    send_group_message_inner(group_id, text, Vec::new(), None).await
}

/// Send a group message that QUOTES another message (tap-to-reply). `reply` rides
/// inside each member's sealed body and is stored on the local message.
pub async fn send_group_message_reply(
    group_id: &str,
    text: &str,
    reply: ReplyRef,
) -> Result<DmMessage, String> {
    send_group_message_inner(group_id, text, Vec::new(), Some(reply)).await
}

/// Send a group message carrying E2E attachments. Upload each file with
/// `upload_attachment` first (once — the CID is shared across members), then
/// pass the refs here; the fan-out seals the refs to each member.
pub async fn send_group_message_with_attachments(
    group_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
) -> Result<DmMessage, String> {
    send_group_message_inner(group_id, text, attachments, None).await
}

/// Send to a group by sealing + fanning out the same body to EACH member over
/// their per-pair channel (no group key — see the module note above).
///
/// WARNING (inline attachments): there is NO shared sealing for the wire. An
/// INLINE attachment (`Attachment.inline_b64`, i.e. a file <=
/// INLINE_ATTACHMENT_MAX_BYTES) is carried in the body, so it gets re-sealed
/// AND re-fragmented INDEPENDENTLY for every member — each pairwise channel is
/// its own PQ seal/ratchet, so the same inline blob crosses N times as N ×
/// (up to ~30) fragments (see the INLINE_ATTACHMENT_MAX_BYTES note for why a
/// max-size inline file is ~30 fragments, not ~8). The 16 KB inline cap bounds
/// this, but large groups should prefer the content-store CID path (a
/// CID/`chunks` attachment uploads ONCE and only the small ref is sealed per
/// member), which keeps fan-out cost flat in file size.
async fn send_group_message_inner(
    group_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
    reply: Option<ReplyRef>,
) -> Result<DmMessage, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() && attachments.is_empty() {
        return Err("empty message".into());
    }
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let mut group = read_groups()
        .await
        .into_iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    // WRITE-BLOCK: a group the admin closed (dissolved) can never be posted to —
    // checked BEFORE the pending auto-accept backstop so nothing can revive it.
    if group.closed {
        return Err("this group was closed by the admin".into());
    }
    if group.pending {
        // Backstop: sending IS consent, so a member who posts auto-joins (the join
        // popup is the primary path via accept_group, but this guarantees a member is
        // never silently blocked if the popup is bypassed).
        group.pending = false;
        let mut all = read_groups().await;
        if let Some(g) = all.iter_mut().find(|g| g.id == group_id) {
            g.pending = false;
        }
        let _ = write_groups(&all).await;
    }
    let plain: String = trimmed.chars().take(4096).collect();

    // Local copy (mine) in the group conversation.
    let msg = DmMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: plain.clone(),
        ts: now_ms(),
        mine: true,
        encrypted: true,
        attachments: attachments.clone(),
        sender_name: me.name.clone(),
        // Our own outgoing group message — author is us (verified by construction).
        sender_did: me.did_key.clone(),
        pinned: false,
        reply_to: reply.clone(),
    };
    let mut conv = read_group_conversation(group_id).await;
    conv.push(msg.clone());
    write_group_conversation(group_id, &conv)
        .await
        .map_err(|e| e.to_string())?;
    let preview = if plain.is_empty() && !attachments.is_empty() {
        format!("📎 {}", attachments[0].name)
    } else {
        plain.clone()
    };
    // Same as the 1:1 path: a hidden control message (SOH-prefixed, e.g. \u{1}hey-gcall:) is
    // protocol traffic — store + fan out, but never let it become the group's last preview.
    if !plain.starts_with('\u{1}') {
        touch_group(group_id, &preview, msg.ts, 0).await;
    }

    // Fan out to every other member. SLIM ROSTER: the full PQ roster (member
    // ML-KEM keys, ~1.2 KB each → past the ~4 KB gossip cap → frag.rs) only needs
    // to travel occasionally, not on every message. Carry the FULL ctx every 16th
    // message (a periodic self-heal for anyone who missed a key update); the rest
    // ride the slim ctx (did+name only). Membership changes already re-announce
    // full via add_group_members, so new members still get keys promptly.
    // `conv` already includes the just-pushed message, so this counts from 1.
    // EAGER FULL-CTX: carry the full PQ roster on the FIRST few messages too,
    // not just every 16th — this self-heals a member that missed the
    // create-announce (so the first thing they receive materialises the group
    // with keys). The rest ride the slim ctx (did+name only).
    let full = conv.len() <= 3 || conv.len() % 16 == 0;
    let ctx = if full {
        group_ctx(&group).await
    } else {
        slim_group_ctx(&group)
    };
    let mut body = if attachments.is_empty() {
        json!({ "text": plain, "group": ctx, "mid": msg.id, "sn": me.name })
    } else {
        json!({ "text": plain, "attachments": attachments, "group": ctx, "mid": msg.id, "sn": me.name })
    };
    if let Some(r) = &reply {
        body["reply"] = reply_ref_json(r);
    }
    // FAN-OUT: join_all the per-member sends. NOTE: on NATIVE these do NOT
    // overlap (fake-async on a current_thread executor) — each member's
    // join_topic_with neighbor wait runs STRUCTURALLY SERIALLY. join_all is
    // harmless on native, a real overlap on wasm; the native latency win comes
    // from the cheap cold-topic gate + faster poll/self-heal + the immediate
    // outbox flush below, NOT from join_all overlap. Errors ignored as before.
    // F-07: never fan out to a barred (kicked/blocked) DID even if the roster
    // still transiently lists it.
    let sends = group
        .members
        .iter()
        .filter(|m| m.did != me.did_key && !is_group_barred(&group, &m.did))
        .map(|m| send_body_to_contact(&m.did, &body));
    let _ = futures_util::future::join_all(sends).await;
    // Push the freshly-enqueued message NOW instead of waiting for the poll.
    crate::api::outbox::flush().await;
    Ok(msg)
}

/// Add more active contacts to an existing group + re-announce the updated
/// roster to every member (so new members materialise the group and existing
/// members learn who joined). Idempotent on already-present members.
pub async fn add_group_members(group_id: &str, new_member_dids: Vec<String>) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let contacts = list_contacts().await;
    let mut groups = read_groups().await;
    let g = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    // Member management is OWNER-gated (only created_by); legacy ownerless groups
    // stay editable by any member for back-compat. (A non-owner's roster change is
    // not authoritative and would be reverted by the owner's next ctx — so we fail
    // loudly here instead of silently no-opping.)
    if !is_group_owner_or_legacy(g, &me.did_key) {
        return Err("only the group owner can add members".into());
    }
    let mut changed = false;
    for did in &new_member_dids {
        if *did == me.did_key || g.members.iter().any(|m| m.did == *did) {
            continue;
        }
        let c = contacts
            .iter()
            .find(|c| c.did == *did && c.is_v2_active())
            .ok_or_else(|| format!("{} is not an active contact", short_did_label(did)))?;
        // An explicit owner/admin re-add clears a prior kick/block barrier for THIS
        // DID (intentional readmission), so the barrier never blocks a deliberate
        // re-invite while still blocking stale-roster re-adds (F-07).
        g.blocked.retain(|b| b != did);
        g.removed.retain(|r| &r.did != did);
        g.members.push(roster_member(c));
        changed = true;
    }
    // Bump the governance epoch on a real roster change so the grow wins anti-
    // rollback at every replica (F-07), even after a prior kick advanced the epoch.
    if changed {
        g.epoch = g.epoch.saturating_add(1);
    }
    let group = g.clone();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    // Re-announce the new roster to ALL members. FAN-OUT: join_all the per-
    // member sends. NOTE: on NATIVE these do NOT overlap (fake-async on a
    // current_thread executor) — they run STRUCTURALLY SERIALLY. join_all is
    // harmless on native, a real overlap on wasm; the native latency win comes
    // from the cheap cold-topic gate + faster poll/self-heal + immediate outbox
    // flush, NOT from join_all overlap. Errors ignored.
    let ctx = group_ctx(&group).await;
    let body = json!({ "group": ctx });
    let sends = group
        .members
        .iter()
        .filter(|m| m.did != me.did_key && !is_group_barred(&group, &m.did))
        .map(|m| send_body_to_contact(&m.did, &body));
    let _ = futures_util::future::join_all(sends).await;
    // Push the freshly-enqueued roster announce NOW so new/removed members
    // materialise on the other devices immediately, not on the next poll cycle.
    crate::api::outbox::flush().await;
    Ok(())
}

/// Pin/unpin a message in a 1-to-1 conversation (local view). HEY chat upgrade.
pub async fn pin_dm_message(did: &str, message_id: &str, pinned: bool) -> Result<(), String> {
    let mut conv = read_conversation(did).await;
    let m = conv
        .iter_mut()
        .find(|m| m.id == message_id)
        .ok_or_else(|| "no such message".to_string())?;
    m.pinned = pinned;
    write_conversation(did, &conv)
        .await
        .map_err(|e| e.to_string())
}

/// Pin/unpin a message in a group conversation (local view). HEY chat upgrade.
pub async fn pin_group_message(
    group_id: &str,
    message_id: &str,
    pinned: bool,
) -> Result<(), String> {
    let mut conv = read_group_conversation(group_id).await;
    let m = conv
        .iter_mut()
        .find(|m| m.id == message_id)
        .ok_or_else(|| "no such message".to_string())?;
    m.pinned = pinned;
    write_group_conversation(group_id, &conv)
        .await
        .map_err(|e| e.to_string())
}

/// Remove (ban) a member from a group. Owner-only. Drops them from the roster and
/// re-announces the updated roster to the REMAINING members so future fan-outs
/// exclude them. No group re-key is needed: group messages fan out to the CURRENT
/// roster over per-pairwise ratchets, so the removed member simply isn't a
/// recipient of anything sent after this (they keep past messages only). HEY
/// chat upgrade.
pub async fn remove_group_member(group_id: &str, member_did: &str) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let mut groups = read_groups().await;
    let g = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    // Member management is OWNER-gated (only created_by). Legacy groups with no
    // recorded owner stay editable by any member for back-compat. (A non-owner kick
    // isn't authoritative — the owner's next ctx would re-add the member — so fail
    // loudly here rather than silently no-op.)
    if !is_group_owner_or_legacy(g, &me.did_key) {
        return Err("only the group owner can remove members".into());
    }
    if member_did == me.did_key {
        return Err("can't remove yourself — delete the group or leave instead".into());
    }
    let before = g.members.len();
    g.members.retain(|m| m.did != member_did);
    if g.members.len() == before {
        return Err("not a member of this group".into());
    }
    // DURABLE removal (F-05): bump the governance epoch and record the kick as a
    // tombstone (removed) AND a hard bar (blocked) AT the new epoch. The bumped
    // epoch wins anti-rollback at every replica, and the barrier means a stale or
    // forged larger roster can NEVER silently re-add this DID (F-07).
    g.epoch = g.epoch.saturating_add(1);
    let new_epoch = g.epoch;
    if !g.removed.iter().any(|r| r.did == member_did) {
        g.removed.push(RemovedMember { did: member_did.to_string(), epoch: new_epoch });
    } else if let Some(r) = g.removed.iter_mut().find(|r| r.did == member_did) {
        r.epoch = new_epoch;
    }
    if !g.blocked.iter().any(|b| b == member_did) {
        g.blocked.push(member_did.to_string());
    }
    let group = g.clone();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    // Re-announce the updated roster so remaining members stop fanning out to the
    // removed member.
    let ctx = group_ctx(&group).await;
    for m in &group.members {
        if m.did == me.did_key || is_group_barred(&group, &m.did) {
            continue;
        }
        let body = json!({ "group": ctx });
        let _ = send_body_to_contact(&m.did, &body).await;
    }
    // Tell the REMOVED member they're out, so the group vanishes on their device
    // (they remove + tombstone it). The recipient honours this only because the
    // signed sender is the group's owner (created_by) — see handle_incoming_group_removed.
    let _ = send_body_to_contact(member_did, &json!({ "group_removed": group_id })).await;
    // Flush so the kick + roster update reach devices now, not on the next poll.
    crate::api::outbox::flush().await;
    Ok(())
}

/// Promote a current member to ADMIN. Admin-gated (owner OR an existing admin may
/// promote). The owner (`created_by`) is implicitly admin and is never listed.
/// Re-announces the updated roster to every member so they all learn the new admin.
pub async fn add_group_admin(group_id: &str, member_did: &str) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let mut groups = read_groups().await;
    let g = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    // OWNER-gated (only created_by); a legacy ownerless group stays editable by any
    // member. A non-owner promotion isn't authoritative (the owner's signed grants
    // are the source of truth, see verify_role_grant) so we fail loudly here.
    if !is_group_owner_or_legacy(g, &me.did_key) {
        return Err("only the group owner can add admins".into());
    }
    // The subject must be a current member (and not the owner, who is already admin).
    if member_did == g.created_by {
        return Err("the group owner is already an admin".into());
    }
    if !g.members.iter().any(|m| m.did == member_did) {
        return Err("not a member of this group".into());
    }
    if !g.admins.iter().any(|a| a == member_did) {
        g.admins.push(member_did.to_string());
        g.epoch = g.epoch.saturating_add(1);
    }
    let group = g.clone();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    // Re-announce the updated roster (carries the new admins list) to all members.
    let ctx = group_ctx(&group).await;
    let body = json!({ "group": ctx });
    for m in &group.members {
        if m.did == me.did_key || is_group_barred(&group, &m.did) {
            continue;
        }
        let _ = send_body_to_contact(&m.did, &body).await;
    }
    Ok(())
}

/// Set the group PICTURE (the CID/ref of a pre-uploaded image — SAME convention as
/// a profile avatar, stored in `avatar_cid`). Admin-gated. Re-announces the roster
/// so every member learns the new picture.
pub async fn set_group_picture(group_id: &str, picture: &str) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let mut groups = read_groups().await;
    let g = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    // OWNER-gated (only created_by); a legacy ownerless group stays editable by any
    // member. The picture is owner-set metadata propagated via the owner's ctx, so a
    // non-owner change would be reverted — fail loudly rather than silently no-op.
    if !is_group_owner_or_legacy(g, &me.did_key) {
        return Err("only the group owner can set the group picture".into());
    }
    g.avatar_cid = if picture.is_empty() {
        None
    } else {
        Some(picture.to_string())
    };
    g.epoch = g.epoch.saturating_add(1);
    let group = g.clone();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    // Re-announce the updated roster (carries avatarCid) to all members.
    let ctx = group_ctx(&group).await;
    let body = json!({ "group": ctx });
    for m in &group.members {
        if m.did == me.did_key || is_group_barred(&group, &m.did) {
            continue;
        }
        let _ = send_body_to_contact(&m.did, &body).await;
    }
    // Push the freshly-enqueued picture announce NOW instead of waiting for the next
    // poll cycle. WITHOUT this, the owner sees the new picture locally but the
    // enqueued announce sits in the outbox until another flush (e.g. the next group
    // message), so other members don't get it. Mirrors the group-message flush.
    crate::api::outbox::flush().await;
    Ok(())
}

/// Store a received group message into its group conversation, materialising the
/// group from the embedded roster first. Deduped by message id. Returns whether
/// a NEW message was appended (false for a redelivery or a roster-only announce).
async fn store_incoming_group_message(
    group_ctx: &Value,
    sender_did: &str,
    text: &str,
    ts: i64,
    dedup_id: Option<&str>,
    attachments: Vec<Attachment>,
    sender_name_hint: Option<&str>,
    reply: Option<ReplyRef>,
) -> Result<bool, String> {
    // Clamp a far-future / non-positive sender ts (security-load-bearing: a forged
    // ts would otherwise pin the group to the top forever / defeat TTL pruning).
    let ts = clamp_recv_ts(ts);
    // Cap the attachment array (anti-DoS); legit messages carry a handful.
    let attachments: Vec<Attachment> = attachments.into_iter().take(MAX_ATTACHMENTS_PER_MSG).collect();
    let Some(gid) = upsert_group_from_ctx(group_ctx, sender_did).await else {
        return Err("group message with no group id".into());
    };
    // Roster-only announce (no text/attachments): group materialised, done.
    if text.is_empty() && attachments.is_empty() {
        return Ok(false);
    }
    let groups = read_groups().await;
    let Some(g) = groups.iter().find(|g| g.id == gid) else {
        return Ok(false);
    };
    // SB-3 membership gate: accept a group message only from a DID in the group's
    // (now owner-controlled) roster — or the owner itself. An outsider who learns
    // the group id / queue cannot inject messages into the conversation.
    // F-07: a kicked/blocked DID is barred — drop its messages even if a stale
    // roster still lists it.
    let is_member = g.created_by == sender_did || g.members.iter().any(|m| m.did == sender_did);
    if !is_member || is_group_barred(g, sender_did) {
        return Ok(false);
    }
    // 1:1 engine block also bars a sender inside a SHARED group — a DID you blocked
    // must not reach you via group fan-out (parity with the DM gate). Non-blocked
    // members are unaffected; group content otherwise still flows.
    if is_blocked(sender_did).await {
        return Ok(false);
    }
    // Muted sender (soft moderation): the message is STORED but never bumps unread
    // or notifies (F-07 muted suppression). Captured here before any roster mutation.
    let sender_muted = g.muted.iter().any(|m| m == sender_did);
    // Prefer the sender's OWN live nickname carried in the message ("sn"); the
    // creator-built roster name is often a generated label. Fall back to the
    // roster name, then a short DID label.
    let hint = sender_name_hint
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let sender_name = hint
        .map(str::to_string)
        .or_else(|| {
            g.members
                .iter()
                .find(|m| m.did == sender_did)
                .map(|m| m.name.clone())
                .filter(|n| !n.is_empty())
        })
        .unwrap_or_else(|| short_did_label(sender_did));

    // Self-heal the roster: if the sender told us a live name that differs from
    // the name we have stored for them, adopt it so group_info / the member list
    // also show the live nickname (not the creator's generated label). Guarded so
    // we only write_groups on an actual change.
    if let Some(live) = hint {
        let mut groups_mut = read_groups().await;
        if let Some(gm) = groups_mut.iter_mut().find(|g| g.id == gid) {
            if let Some(m) = gm.members.iter_mut().find(|m| m.did == sender_did) {
                if m.name != live {
                    m.name = live.to_string();
                    let _ = write_groups(&groups_mut).await;
                }
            }
        }
    }

    // O(1) fast-path dedup BEFORE the disk read (redeliveries are routine). The
    // durable log scan below remains the source of truth (survives restart).
    if let Some(id) = dedup_id {
        if dedup_seen(&format!("g:{gid}"), id) {
            return Ok(false); // redelivery
        }
    }
    let mut conv = read_group_conversation(&gid).await;
    if let Some(id) = dedup_id {
        if conv.iter().any(|m| m.id == id) {
            return Ok(false); // redelivery
        }
    }
    let msg = DmMessage {
        id: dedup_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        text: text.chars().take(4096).collect(),
        ts,
        mine: false,
        encrypted: true,
        attachments,
        sender_name: sender_name.clone(),
        // VERIFIED author: this `sender_did` is the caller's `inner.sender_did` /
        // ratchet-bound contact DID that `verify_inner` authenticated — never a
        // self-asserted payload field. Load-bearing for the group-call roster.
        sender_did: sender_did.to_string(),
        pinned: false,
        reply_to: reply,
    };
    let preview = if msg.text.is_empty() && !msg.attachments.is_empty() {
        format!("{}: 📎 {}", sender_name, msg.attachments[0].name)
    } else {
        format!("{sender_name}: {}", msg.text)
    };
    conv.push(msg);
    // LOCAL RETENTION BOUND: keep only the newest MAX_CONV_MSGS (oldest pruned).
    // Local-only — never signals peers; the dropped tail is old stored history.
    cap_conv_log(&mut conv);
    write_group_conversation(&gid, &conv)
        .await
        .map_err(|e| e.to_string())?;
    // Group call-control ("hey-gcall") signals are stored so call_poll can read the ring,
    // but they are not chat messages: skip the unread bump or they leak as a badge /
    // "New messages" notification (already hidden from the rendered thread).
    let lane = text.strip_prefix('\u{1}').unwrap_or(text);
    if lane.starts_with("hey-gcall:1:") || lane.starts_with("hey-call:1:") {
        return Ok(false);
    }
    // Muted sender: store + refresh preview/ts but DON'T bump unread (no notify).
    let unread_delta = if sender_muted { 0 } else { 1 };
    touch_group(&gid, &preview, ts, unread_delta).await;
    Ok(true)
}

/// Mark a group's messages read.
pub async fn mark_group_read(group_id: &str) {
    let mut groups = read_groups().await;
    if let Some(g) = groups.iter_mut().find(|g| g.id == group_id) {
        if g.unread != 0 {
            g.unread = 0;
            let _ = write_groups(&groups).await;
        }
    }
}

/// Delete a group locally (record + message log). Local-only; other members keep
/// the group. You stop receiving its messages (they still arrive as DMs from
/// members, but with no local group they re-materialise it — leave the group
/// roster instead if you want it gone for good, a follow-up).
pub async fn delete_group(group_id: &str) -> Result<(), String> {
    let groups: Vec<Group> = read_groups()
        .await
        .into_iter()
        .filter(|g| g.id != group_id)
        .collect();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    let _ = storage::remove(&group_conv_path(group_id)).await;
    // Tombstone the id (the same guard `decline_group` uses) so a still-in-flight
    // roster from another member can't RE-MATERIALISE the group we just deleted.
    // Without this, leaving/deleting a group never sticks — the next member message
    // re-creates it (and re-asserts its members as contacts).
    let mut declined = read_declined_groups().await;
    if !declined.iter().any(|d| d == group_id) {
        declined.push(group_id.to_string());
        if let Ok(v) = serde_json::to_value(&declined) {
            let _ = storage::write_json(DECLINED_GROUPS_FILE, &v).await;
        }
    }
    Ok(())
}

/// ADMIN "delete group for everyone": only the CREATOR may dissolve a group.
/// Fans a signed DISSOLVE control to every other member (same per-pair fan-out
/// as a group message / reaction), then deletes it locally + tombstones the id
/// (so a still-in-flight roster can't re-materialise it). Each recipient honours
/// the dissolve ONLY if its signed sender is that group's `created_by`, so a
/// non-creator can never tear down the group for everyone.
pub async fn dissolve_group(group_id: &str) -> Result<(), String> {
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    let group = read_groups()
        .await
        .into_iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| "no such group".to_string())?;
    // AUTHORITY CHECK — only the creator may dissolve for everyone. A legacy
    // ownerless group (empty created_by) has no enforceable owner, so we allow
    // the local delete to proceed for it (no remote can be authoritatively told).
    if !group.created_by.is_empty() && me.did_key != group.created_by {
        return Err("only the group creator can delete for everyone".into());
    }
    // Fan the DISSOLVE control to every other member over their per-pair channel.
    let ctx = group_ctx(&group).await;
    let body = json!({ "dissolve": true, "group": ctx });
    for m in &group.members {
        if m.did == me.did_key || is_group_barred(&group, &m.did) {
            continue;
        }
        let _ = send_body_to_contact(&m.did, &body).await;
    }
    // Flush so the dissolve reaches members before we delete locally below — without
    // it the enqueued control could be dropped when local state is torn down.
    crate::api::outbox::flush().await;
    // Delete locally + tombstone (same logic as delete_group): remove from
    // groups, drop the conversation, add the id to declined-groups so an
    // in-flight roster can't re-materialise it.
    let groups: Vec<Group> = read_groups()
        .await
        .into_iter()
        .filter(|g| g.id != group_id)
        .collect();
    write_groups(&groups).await.map_err(|e| e.to_string())?;
    let _ = storage::remove(&group_conv_path(group_id)).await;
    let mut declined = read_declined_groups().await;
    if !declined.iter().any(|d| d == group_id) {
        declined.push(group_id.to_string());
        if let Ok(v) = serde_json::to_value(&declined) {
            let _ = storage::write_json(DECLINED_GROUPS_FILE, &v).await;
        }
    }
    Ok(())
}

/// Honour an inbound `group_removed` control: the group OWNER kicked this member.
/// Honoured ONLY if the cryptographically-signed `sender_did` equals the locally
/// recorded group's `created_by` (a non-owner can't kick you). When honoured, the
/// group is fully REMOVED locally and its id tombstoned (declined-groups) so a
/// stale in-flight roster can't re-materialise it — the removed member then sees
/// nothing (gone from the list, no messages, can't write). If the group isn't held
/// locally or the sender isn't the owner, the control is ignored (still consumed,
/// never stored). A later legitimate re-invite + accept clears the tombstone.
async fn handle_incoming_group_removed(gid: &str, sender_did: &str) -> bool {
    let groups = read_groups().await;
    let authorized = groups
        .iter()
        .find(|g| g.id == gid)
        .map(|g| !g.created_by.is_empty() && g.created_by == sender_did)
        .unwrap_or(false);
    if !authorized {
        return true; // unknown group, or not from the owner — ignore, never store
    }
    let remaining: Vec<Group> = groups.into_iter().filter(|g| g.id != gid).collect();
    let _ = write_groups(&remaining).await;
    let _ = storage::remove(&group_conv_path(gid)).await;
    let mut declined = read_declined_groups().await;
    if !declined.iter().any(|d| d == gid) {
        declined.push(gid.to_string());
        if let Ok(v) = serde_json::to_value(&declined) {
            let _ = storage::write_json(DECLINED_GROUPS_FILE, &v).await;
        }
    }
    true
}

/// Honour an inbound DISSOLVE control (`body.dissolve == true`). Returns true if
/// the control was CONSUMED (caller must NOT store it as a chat message). The
/// dissolve is honoured ONLY when the cryptographically-signed `sender_did`
/// equals the locally-recorded group's `created_by` (a non-creator's dissolve is
/// ignored — security). When honoured, the local group is marked `closed` (kept
/// for read-only history) and persisted. If the group isn't held locally the
/// control is simply ignored (still consumed, never stored).
async fn handle_incoming_dissolve(group_ctx: &Value, sender_did: &str) -> bool {
    let Some(gid) = group_ctx.get("id").and_then(|v| v.as_str()) else {
        return true; // malformed — consume, never store
    };
    let mut groups = read_groups().await;
    if let Some(g) = groups.iter_mut().find(|g| g.id == gid) {
        if !g.created_by.is_empty() && g.created_by == sender_did {
            if !g.closed {
                g.closed = true;
                let _ = write_groups(&groups).await;
            }
        }
        // else: a non-creator dissolve, or an ownerless legacy group — ignore.
    }
    true
}

/// Extract the queue id from a `hey-v0/q/<id>` topic. Returns None if
/// the topic doesn't match the expected shape.
fn queue_id_from_topic(topic: &str) -> Option<&str> {
    topic.strip_prefix(&format!("{TOPIC_PREFIX_V2}/"))
}

/// Receive a v2 (sealed-sender) DM from a per-pair queue topic. Called
/// by peer_receiver when it pulls a wire entry from `hey-v0/q/<id>`.
///
/// The provider has handed us the wire string (`{ type: "dm.v2",
/// envelope }`) and the topic it came from. We decrypt the envelope,
/// verify the inner signature, and dispatch on inner.kind. For
/// handshakes we resolve the pending contact by the queue id (since
/// the sender's real DID was previously unknown to us).
pub async fn receive_v2_wire(topic: &str, wire: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(wire).map_err(|e| format!("wire json: {e}"))?;
    if v.get("type").and_then(|t| t.as_str()) != Some("dm.v2") {
        return Err("not a dm.v2 wire".into());
    }
    let env_val = v.get("envelope").ok_or_else(|| "no envelope".to_string())?;
    let envelope: HpqEnvelope =
        serde_json::from_value(env_val.clone()).map_err(|e| format!("envelope shape: {e}"))?;
    let queue_id = queue_id_from_topic(topic);

    // A cleartext `rh` (the page number) marks a Double Ratchet message — always
    // a KIND_MESSAGE. Control messages (handshake/welcome) never carry rh and
    // go down the single-shot path below.
    if let Some(rh) = v.get("rh") {
        let pn = u32_field(rh, "pn")?;
        let n = u32_field(rh, "n")?;
        let kc = rh.get("kc").and_then(|x| x.as_str()).map(String::from);
        let kp = rh.get("kp").and_then(|x| x.as_str()).map(String::from);
        return receive_ratchet_message(queue_id, &envelope, pn, n, kc, kp).await;
    }

    // No rh ⇒ single-shot. Anonymous contacts seal to a per-contact ephemeral
    // pubkey, so the decrypt keys are chosen by the queue this landed on.
    let via = decrypt_via_for_queue(queue_id).await?;
    let inner = decrypt_envelope_to_inner(&envelope, &via).await?;
    // F-08: verify against the recipient (=us) + conversation (=this queue)
    // bound form; `verify_inner_bound` falls back to the legacy form, so control
    // messages (handshake/welcome, signed unbound) and not-yet-upgraded peers
    // still verify and delivery is never broken.
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if !verify_inner_bound(&inner, Some(&my_did), queue_id) {
        return Err("inner signature mismatch".into());
    }
    match inner.kind.as_str() {
        KIND_MESSAGE => {
            let queue_id = queue_id.ok_or_else(|| "bad topic".to_string())?;
            // Defense in depth: the sender_did must own the queue this landed
            // on. Stops a stranger delivering via a leaked queue id. Accept the
            // minted queue OR the deterministic per-pair queue (cross-runtime).
            let owner = list_contacts().await.into_iter().find(|c| {
                c.did == inner.sender_did
                    && c.owns_inbound_queue_with(queue_id, &my_did)
                    && c.status == ContactStatus::Active
            });
            let owner = owner.ok_or_else(|| "sender does not match queue owner".to_string())?;
            // F-11: learn whether this peer supports the salted topic so future
            // sends migrate off the leaky deterministic topic.
            note_peer_salted(&inner.sender_did, &inner.body).await;
            // Downgrade protection (must-fix #6): a ratchet-capable contact must
            // never be served a single-shot message — refuse rather than fall
            // back to the no-PCS path (the OOB invite is only TOFU-authenticated).
            if owner.ratchet_capable {
                return Err(
                    "refusing single-shot message from a ratchet-capable contact (downgrade)"
                        .into(),
                );
            }
            let text = inner
                .body
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let atts = attachments_from_body(&inner.body);
            let dedup_id = single_shot_dedup_id(&envelope);
            // Reaction rides in the body — apply it and stop (no message stored).
            if let Some(react) = inner.body.get("reaction") {
                return handle_incoming_reaction(&inner, react).await;
            }
            // HIDDEN profile-name control: the sender edited their nickname and is
            // pushing it so chat refreshes immediately. Update the contact name and
            // stop — never stored as a visible message.
            if let Some(pn) = inner.body.get("profile_name").and_then(Value::as_str) {
                refresh_contact_name(&inner.sender_did, pn).await;
                return Ok(());
            }
            // HIDDEN removal control: the group OWNER kicked this member — the group
            // vanishes locally (removed + tombstoned). Honoured only if the signed
            // sender is the group's creator. Consumed — never stored as a message.
            if let Some(gid) = inner.body.get("group_removed").and_then(Value::as_str) {
                handle_incoming_group_removed(gid, &inner.sender_did).await;
                return Ok(());
            }
            // GROUP message? route to the group conversation (materialising the
            // group from the embedded roster), not the 1-to-1.
            if let Some(group_ctx) = inner.body.get("group") {
                // ADMIN "delete for everyone": honoured only if the SIGNED sender
                // is the group's creator. Consumed — never stored as a message.
                if inner.body.get("dissolve").and_then(Value::as_bool) == Some(true) {
                    handle_incoming_dissolve(group_ctx, &inner.sender_did).await;
                    return Ok(());
                }
                let shared = inner.body.get("mid").and_then(Value::as_str).map(str::to_string);
                let gid_store = shared.as_deref().unwrap_or(&dedup_id);
                store_incoming_group_message(
                    group_ctx,
                    &inner.sender_did,
                    text,
                    inner.ts,
                    Some(gid_store),
                    atts,
                    inner.body.get("sn").and_then(Value::as_str),
                    parse_reply_ref(&inner.body),
                )
                .await?;
                return Ok(());
            }
            // Verse lane: ephemeral — never stored as a message.
            if let Some(vp) = text.strip_prefix(VERSE_PREFIX) {
                // F-VERSE-BLOCK-BYPASS: drop a blocked sender's presence/location frame, mirroring
                // the DM is_blocked gate — a blocked peer's verse activity must not reach the viewer.
                if !is_blocked(&inner.sender_did).await {
                    verse_push(&inner.sender_did, vp);
                }
                return Ok(());
            }
            if text.is_empty() && atts.is_empty() {
                return Err("message body has neither text nor attachments".into());
            }
            let shared_id = inner.body.get("mid").and_then(Value::as_str).map(str::to_string);
            let store_id = shared_id.as_deref().unwrap_or(&dedup_id);
            // The sender's live nickname ("sn") is folded into the store's single
            // contacts RMW (coalesced) instead of a separate refresh_contact_name write.
            let sn = inner
                .body
                .get("sn")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("");
            let appended =
                store_incoming_message(&inner.sender_did, text, inner.ts, Some(store_id), atts, sn, parse_reply_ref(&inner.body))
                    .await?;
            if appended {
                maybe_rotate_inbound_queue(&inner.sender_did).await;
            }
            // Self-updating friendsbook: keep the sender's latest node ticket.
            // The sender_did is VERIFIED here (verify_inner_bound + queue-owner
            // check above), and it's asserting its OWN endpoint ⇒ trusted dial
            // anchor (self_asserted=true).
            if let Some(nt) = inner.body.get("nt").and_then(Value::as_str) {
                refresh_peer_ticket(&inner.sender_did, nt, true).await;
            }
            Ok(())
        }
        KIND_HANDSHAKE => {
            let queue_id = queue_id.ok_or_else(|| "bad topic".to_string())?;
            receive_handshake(&inner, queue_id).await
        }
        KIND_WELCOME => receive_welcome(&inner).await,
        other => Err(format!("unknown inner kind: {other}")),
    }
}

/// Parse a non-negative u32 wire field, rejecting anything out of range BEFORE
/// it can reach the ratchet (a forged 2^40 counter must not wrap to a small u32
/// and slip under the MAX_SKIP cap).
fn u32_field(obj: &Value, key: &str) -> Result<u32, String> {
    obj.get(key)
        .and_then(|x| x.as_u64())
        .filter(|&x| x <= u32::MAX as u64)
        .map(|x| x as u32)
        .ok_or_else(|| format!("rh missing or out-of-range {key}"))
}

/// Deterministic per-message id derived from the sealed ciphertext. Identical
/// across redeliveries of the SAME envelope (fresh nonce/KEM make distinct
/// messages differ), so it dedups the non-idempotent ratchet advance.
fn ratchet_dedup_id(env: &HpqEnvelope) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(env.ct.as_bytes());
    h.update(env.n.as_bytes());
    format!("rdm:{}", bytes_to_hex(&h.finalize()[..16]))
}

/// Deterministic dedup id for a SINGLE-SHOT (non-ratchet) v2 message, derived
/// from the sealed envelope (ct + nonce) — same construction as
/// `ratchet_dedup_id`, distinct prefix. The outbox now retries until delivery
/// is confirmed and gossip can re-deliver, so a redelivered identical envelope
/// must no-op rather than append a duplicate line. (The ratchet path already
/// dedups via `ratchet_dedup_id`; this closes the same gap for single-shot.)
fn single_shot_dedup_id(env: &HpqEnvelope) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(env.ct.as_bytes());
    h.update(env.n.as_bytes());
    format!("sdm:{}", bytes_to_hex(&h.finalize()[..16]))
}

/// Deterministic per-direction inbound queue id from the (recipient, sender)
/// DID pair. Both peers compute the IDENTICAL id for a direction (recipient
/// receives FROM sender), so the sender's send topic and the recipient's listen
/// topic ALWAYS converge — WITHOUT the mint-and-advertise handshake that was
/// desyncing and silently stranding DMs on a queue the recipient never joined
/// (the cross-runtime DM bug). Only for Regular-mode contacts (both know real
/// DIDs); Anonymous contacts keep the advertised minted queue (the peer can't
/// derive this without our real DID). Metadata trade-off: derivable by anyone
/// holding BOTH DIDs; message CONTENT stays sealed-sender E2E.
fn pair_inbound_queue(recipient_did: &str, sender_did: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"hey-dm-pair-inbound-v1\0");
    h.update(recipient_did.as_bytes());
    h.update(b"\0");
    h.update(sender_did.as_bytes());
    bytes_to_hex(&h.finalize()[..])
}

/// F-11: SALTED per-pair topic. The legacy `pair_inbound_queue` is
/// SHA256(DID‖DID) — anyone holding both DIDs can compute it (a metadata leak:
/// an observer can watch a known pair's traffic, even though CONTENT stays E2E).
/// This salts the topic with the per-pair X25519 STATIC-STATIC shared secret
/// (`DH(my_priv, peer_static_pub) == DH(peer_priv, my_static_pub)`), which only
/// the two key-holders can compute. The DIDs are folded in SORTED order so BOTH
/// peers derive the IDENTICAL topic regardless of who is "recipient". `x_shared`
/// is the 32-byte X25519 ECDH output (provider- or locally-computed). Direction-
/// independent: the same topic carries both directions (the legacy queue was
/// per-direction, but the salted queue need not be — both sides listen+send on
/// the one salted topic). This is an ADDITIONAL topic; the legacy one stays a
/// guaranteed-deliverable fallback so a not-yet-upgraded peer is never stranded.
fn salted_pair_queue(my_did: &str, peer_did: &str, x_shared: &[u8]) -> String {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let (lo, hi) = if my_did <= peer_did {
        (my_did, peer_did)
    } else {
        (peer_did, my_did)
    };
    let mut info = Vec::with_capacity(lo.len() + hi.len() + 1);
    info.extend_from_slice(lo.as_bytes());
    info.push(0);
    info.extend_from_slice(hi.as_bytes());
    let hk = Hkdf::<Sha256>::new(Some(b"hey-dm-pair-salted-v1"), x_shared);
    let mut out = [0u8; 32];
    // okm length matches the legacy 32-byte topic; expand can't fail for 32 bytes.
    hk.expand(&info, &mut out)
        .expect("hkdf expand 32 bytes never fails");
    bytes_to_hex(&out)
}

/// The X25519 static-static shared secret with this contact, or None if we can't
/// derive it (no cached peer keys, or the key bytes are malformed). Regular
/// contacts use the provider-held session key (the wallet model — the private
/// key never leaves the runtime); Anonymous contacts use their per-contact local
/// ephemeral key. Keyless/feed-only contacts (no `peer_pubkeys`) return None and
/// stay on the legacy deterministic topic.
async fn pair_x25519_shared(c: &DmContact) -> Option<Vec<u8>> {
    let peer = c.peer_pubkeys.as_ref()?;
    let peer_x: [u8; 32] = B64.decode(&peer.x25519_pub_b64).ok()?.try_into().ok()?;
    let via = decrypt_via_for_contact(c).ok()?;
    match via {
        DecryptVia::Local(keys) => Some(crypto::dh(&keys.x25519_priv, &peer_x).to_vec()),
        DecryptVia::Provider => {
            let resp = crate::runtime::identity_provider::x25519_dh(IDENTITY_NS, &peer_x)
                .await
                .ok()?;
            crate::runtime::identity_provider::shared_from(&resp).ok()
        }
    }
}

/// Return this contact's cached salted topic, deriving + persisting it on first
/// use (the derivation needs an async DH, so we pin the result to keep the sync
/// ownership check + the listen/send paths cheap). None ⇒ not derivable
/// (keyless/feed-only) ⇒ caller stays on the legacy deterministic topic. Only
/// meaningful for Regular contacts (Anonymous keep their minted advertised queue
/// — the peer can't derive the legacy pair topic, so there's no leak to fix).
async fn ensure_salted_queue(did: &str) -> Option<String> {
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if my_did.is_empty() {
        return None;
    }
    let c = find_contact(did).await?;
    if !matches!(c.mode, IdentityMode::Regular) {
        return None;
    }
    if let Some(q) = c.salted_queue.clone() {
        // F-LEGACY-PAIR-TOPIC (re-fix): an EXISTING roster may already have
        // `salted_queue` pinned (from before this re-fix) but
        // `salted_self_ready_at == 0` (serde-default) — in which case the early
        // return below would never start the SELF-owned grace clock and the legacy
        // topic would leak forever. Stamp it once here so already-migrated contacts
        // also abandon the legacy subscription after a bounded window.
        if c.salted_self_ready_at == 0 {
            let _g = contacts_gate().lock().await;
            let mut list = list_contacts().await;
            if let Some(rec) = list.iter_mut().find(|r| r.did == did) {
                if rec.salted_self_ready_at == 0 {
                    rec.salted_self_ready_at = now_ms();
                    let _ = write_contacts(&list).await;
                }
            }
        }
        return Some(q);
    }
    let x_shared = pair_x25519_shared(&c).await?;
    let salted = salted_pair_queue(&my_did, &c.did, &x_shared);
    // Pin it so subsequent ownership checks / listen / send reuse it without a DH.
    // The continuity-pin RMW is serialized against the rest of the receive path
    // (the lost-update race the gate closes) — the async DH above stays OUTSIDE it.
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    if let Some(rec) = list.iter_mut().find(|r| r.did == did) {
        let mut dirty = false;
        if rec.salted_queue.as_deref() != Some(salted.as_str()) {
            rec.salted_queue = Some(salted.clone());
            dirty = true;
        }
        // F-LEGACY-PAIR-TOPIC (re-fix): stamp the SELF-owned "we have our salted
        // topic" moment the first time we pin it. This drives the legacy-topic
        // LISTEN abandonment grace independently of whether the peer ever
        // advertises `sc:true`, so a non-cooperating peer can't keep us
        // subscribed to the DID-derivable legacy pair topic forever.
        if rec.salted_self_ready_at == 0 {
            rec.salted_self_ready_at = now_ms();
            dirty = true;
        }
        if dirty {
            let _ = write_contacts(&list).await;
        }
    }
    Some(salted)
}

/// True if the conversation with `sender` already holds a message with `id`.
async fn conv_has(sender: &str, id: &str) -> bool {
    read_conversation(sender).await.iter().any(|m| m.id == id)
}

/// Append a received message to its conversation + bump the contact preview.
/// When `dedup_id` is set, a message already bearing that id is treated as a
/// redelivery and NOT re-appended (the caller still persists ratchet state).
/// Returns `true` if a NEW message was appended, `false` on a redelivery —
/// the caller uses that to count rotation-eligible messages exactly once.
async fn store_incoming_message(
    sender_did: &str,
    text: &str,
    ts: i64,
    dedup_id: Option<&str>,
    attachments: Vec<Attachment>,
    sender_name: &str,
    reply: Option<ReplyRef>,
) -> Result<bool, String> {
    // F-BLOCK-CALL-RING: fail closed on a blocked sender BEFORE any store, notify,
    // or conversation-create. `sender_did` here is the VERIFIED author (the
    // ratchet-bound contact DID / `inner.sender_did` that `verify_inner`
    // authenticated — never a self-asserted payload field), so a blocked peer
    // cannot spoof past this. Returns Ok(false) ("not appended"), the same signal
    // the hidden-control path returns, so callers skip queue rotation. This also
    // drops the `\u{1}hey-call:1:` ring control message (it never reaches the
    // conversation log that social.rs call_poll scans), so a blocked DID can't
    // ring the device. Unblocking restores delivery of NEW messages immediately.
    if is_blocked(sender_did).await {
        return Ok(false);
    }
    // Clamp a far-future / non-positive sender ts so it can't pin this
    // conversation to the top forever or defeat TTL pruning (security-load-bearing).
    let ts = clamp_recv_ts(ts);
    // Cap the attachment array so a forged message can't pin us into thousands of
    // fetches. Legit messages carry a handful; this only bites a hostile payload.
    let attachments: Vec<Attachment> = attachments.into_iter().take(MAX_ATTACHMENTS_PER_MSG).collect();
    // Protocol lanes must NEVER land as conversation text. Every incoming
    // store passes through here, so this catches a verse handshake arriving
    // via ANY path — direct, queued, frag-reassembled, or a peer build that
    // dropped the \u{1} control byte (the leak users saw as
    // "hey-verse:1:eyJr…" bubbles).
    let lane = text.strip_prefix('\u{1}').unwrap_or(text);
    if let Some(vp) = lane.strip_prefix("hey-verse:1:") {
        verse_push(sender_did, vp);
        return Ok(false);
    }
    // Call-control DMs (1:1 "hey-call" + group "hey-gcall") ride the DM transport so
    // call_poll can read the ring from the stored conversation, but they are NOT chat
    // messages: store them, but do NOT touch contact metadata (no unread bump, no preview,
    // no last-ts) or they leak as a dock badge + a "New messages" notification. They are
    // already hidden from the rendered thread by the social.rs chat_conversation filter.
    // ANY hidden control message — call/gcall ring signals, address cards (hey-addr),
    // edits, deletes — is \u{1}-prefixed and is filtered out of the rendered thread by
    // chat_conversation. Store it (its handler reads it back from the log: call_poll /
    // address-card cache / edit / delete) but NEVER bump unread or the preview, or it
    // leaks as a dock badge + "New messages" with nothing to actually read.
    let is_hidden_ctrl = text.starts_with('\u{1}');
    // O(1) fast-path dedup BEFORE the disk read (redeliveries are routine). The
    // durable log scan below remains the source of truth (survives restart).
    if let Some(id) = dedup_id {
        if dedup_seen(sender_did, id) {
            return Ok(false); // redelivery — already stored
        }
    }
    let mut conv = read_conversation(sender_did).await;
    if let Some(id) = dedup_id {
        if conv.iter().any(|m| m.id == id) {
            return Ok(false); // redelivery — already stored
        }
    }
    let msg = DmMessage {
        id: dedup_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        text: text.chars().take(4096).collect(),
        ts,
        mine: false,
        encrypted: true,
        attachments,
        sender_name: String::new(),
        // VERIFIED author: this `sender_did` is the caller's `inner.sender_did` /
        // ratchet-bound contact DID that `verify_inner` authenticated — never a
        // self-asserted payload field. Load-bearing for tombstone/delete authority.
        sender_did: sender_did.to_string(),
        pinned: false,
        reply_to: reply,
    };
    let preview = if msg.text.is_empty() && !msg.attachments.is_empty() {
        format!("📎 {}", msg.attachments[0].name)
    } else {
        msg.text.clone()
    };
    conv.push(msg.clone());
    // LOCAL RETENTION BOUND: keep only the newest MAX_CONV_MSGS on the receive
    // write path (oldest pruned). Local-only — never signals peers; the dropped
    // tail is old, already-stored INCOMING history; outbound is unaffected.
    cap_conv_log(&mut conv);
    write_conversation(sender_did, &conv)
        .await
        .map_err(|e| e.to_string())?;
    if is_hidden_ctrl {
        // Stored for its handler (call_poll / address-card cache / edit / delete);
        // no unread bump / preview / notification.
        return Ok(false);
    }
    // CHAT-CAPABILITY: a real inbound USER message means the sender DELIBERATELY chose to chat us.
    // ALWAYS surface it (we NEVER silently hide a real inbound message — that lost messages when a
    // contact wasn't yet chat-enabled, e.g. a cross-version peer or a race), and ENABLE chat so we
    // can reply. This is self-healing: even if `enable_chat` never ran during establishment, the
    // first inbound message enables the thread. Isolation still holds: a normal app CANNOT send to a
    // follow-only contact (the SEND gate blocks it), so a follow alone never produces an inbound here
    // — and control DMs (follow announce / feed key) returned above, so they never enable chat. Only
    // a genuine person who already holds our keys and chose to message us opens the thread.
    enable_chat(sender_did).await;
    // Coalesced: preview/unread bump AND the live nickname adopt in ONE RMW.
    touch_contact_message_named(sender_did, sender_name, &preview, msg.ts, 1)
        .await
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Receive a Double Ratchet message. The whole ratchet advance runs on a CLONE
/// and is committed only after an AUTHENTICATED decrypt + signature + header
/// check — so a forged/replayed/old-epoch envelope can never corrupt the
/// committed ratchet (it just fails and the clone is dropped). On success the
/// plaintext is stored FIRST and the advanced ratchet persisted LAST, so a
/// crash in between is healed by redelivery (re-derive + dedup) rather than
/// losing the message (must-fix #4).
async fn receive_ratchet_message(
    queue_id: Option<&str>,
    envelope: &HpqEnvelope,
    pn: u32,
    n: u32,
    kem_ct_b64: Option<String>,
    kem_pub_b64: Option<String>,
) -> Result<(), String> {
    let queue_id = queue_id.ok_or_else(|| "bad topic".to_string())?;
    // The contact owning this inbound queue — its did is the peer's signing did
    // AND the key the ratchet state is filed under. Match the minted queue OR
    // the deterministic per-pair queue (Regular contacts' cross-runtime path).
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    let c = list_contacts()
        .await
        .into_iter()
        .find(|c| c.owns_inbound_queue_with(queue_id, &my_did) && c.status == ContactStatus::Active)
        .ok_or_else(|| "ratchet message on an unowned queue".to_string())?;
    if !c.ratchet_capable {
        return Err("ratchet message for a non-ratchet contact".into());
    }
    let st0 = read_ratchet(&c.did)
        .await
        .filter(|s| s.is_bootstrapped())
        .ok_or_else(|| "ratchet message but no ratchet state".to_string())?;

    // dh comes from the UNSEALED envelope.eph (and is re-checked against the
    // sealed+signed header after decrypt).
    let dh_bytes: [u8; 32] = B64
        .decode(&envelope.eph)
        .map_err(|e| format!("eph b64: {e}"))?
        .try_into()
        .map_err(|_| "eph wrong size".to_string())?;
    let dh_hex = hx(&dh_bytes);
    let dedup_id = ratchet_dedup_id(envelope);

    // No pre-decrypt short-circuit, and we NEVER store anything that didn't open
    // + verify: a message we can't decrypt is indistinguishable from a forgery
    // (the relay/anyone who learns the queue id can craft one with the peer's
    // public eph + any n), so storing an "undecryptable" marker for it would let
    // them inject unauthenticated lines into the conversation. Instead the whole
    // advance runs on a CLONE that is committed only after an AUTHENTICATED
    // decrypt+verify; if that fails we either no-op a genuine redelivery (the
    // conversation already holds this exact ciphertext) or return an explicit
    // Err (logged by the receive loop) — never a silent Ok, never a silent drop.
    // A genuinely lost message (its skipped key was evicted by TTL/FIFO before it
    // arrived) lands here too: surfaced as a logged Err, not invented UI.
    let mut st = st0.clone();
    let mk = match ratchet_step_recv(
        &mut st,
        &dh_hex,
        dh_bytes,
        pn,
        n,
        kem_ct_b64.as_deref(),
        kem_pub_b64.as_deref(),
    ) {
        Ok(mk) => mk,
        Err(e) => {
            if conv_has(&c.did, &dedup_id).await {
                return Ok(()); // redelivery of an already-stored message — benign
            }
            return Err(format!(
                "ratchet advance (undecryptable/forged, dropped): {e}"
            ));
        }
    };
    // Wipe the transient message key on scope exit (L: transient AEAD key not
    // zeroized). Derefs to `[u8;32]`, so `&mk` into open_with_secrets is unchanged.
    let mk = zeroize::Zeroizing::new(mk);
    let via = decrypt_via_for_contact(&c)?;
    let kem_ss = ratchet_kem_ss(envelope, &via).await?;
    let plaintext = match crypto::open_with_secrets(envelope, &*mk, &kem_ss) {
        Ok(pt) => pt,
        Err(e) => {
            if conv_has(&c.did, &dedup_id).await {
                return Ok(());
            }
            return Err(format!(
                "ratchet decrypt (undecryptable/forged, dropped): {e}"
            ));
        }
    };

    let inner: InnerPayload =
        serde_json::from_str(&plaintext).map_err(|e| format!("inner deserialize: {e}"))?;
    // F-08: verify against the recipient (=us) + conversation (=this queue) bound
    // form, falling back to the legacy form for not-yet-upgraded peers.
    if !verify_inner_bound(&inner, Some(&my_did), Some(queue_id)) {
        return Err("inner signature mismatch".into());
    }
    // The sealed+signed header must match the eph we keyed on, the cleartext
    // page number we advanced to, AND the cleartext rolling-KEM fields — closes
    // any wire/seal tampering (incl. a stripped/forged kem_ct downgrade).
    let want = RatchetHeader {
        dh: envelope.eph.clone(),
        pn,
        n,
        kem_ct: kem_ct_b64.clone(),
        kem_pub: kem_pub_b64.clone(),
    };
    if inner.rh.as_ref() != Some(&want) {
        return Err("ratchet header mismatch (sealed vs wire)".into());
    }
    if inner.kind != KIND_MESSAGE {
        return Err("ratchet wire carried a non-message kind".into());
    }
    if inner.sender_did != c.did {
        return Err("ratchet sender does not match queue owner".into());
    }
    // F-11: learn whether this peer supports the salted topic so future sends
    // migrate off the leaky deterministic topic.
    note_peer_salted(&c.did, &inner.body).await;
    let text = inner
        .body
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let atts = attachments_from_body(&inner.body);

    // Reaction rides in the body — apply it, persist the ratchet advance, stop.
    if let Some(react) = inner.body.get("reaction") {
        handle_incoming_reaction(&inner, react).await?;
        write_ratchet(&c.did, &st).await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    // HIDDEN profile-name control: refresh the contact name, persist the ratchet
    // advance (this WAS a real ratchet message), and stop — never stored.
    if let Some(pn) = inner.body.get("profile_name").and_then(Value::as_str) {
        refresh_contact_name(&c.did, pn).await;
        write_ratchet(&c.did, &st).await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    // HIDDEN removal control: the group OWNER kicked this member — the group
    // vanishes locally (removed + tombstoned). Honoured only if the signed sender
    // is the creator. The ratchet still advanced, so persist it. Never stored.
    if let Some(gid) = inner.body.get("group_removed").and_then(Value::as_str) {
        handle_incoming_group_removed(gid, &c.did).await;
        write_ratchet(&c.did, &st).await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    // GROUP message? route to the group conversation. The ratchet still advanced
    // (this WAS a real ratchet message), so persist the consumed state regardless.
    if let Some(group_ctx) = inner.body.get("group") {
        // ADMIN "delete for everyone": honoured only if the SIGNED sender is the
        // group's creator. Consumed — never stored — but the ratchet still
        // advanced, so persist the consumed state.
        if inner.body.get("dissolve").and_then(Value::as_bool) == Some(true) {
            handle_incoming_dissolve(group_ctx, &c.did).await;
            write_ratchet(&c.did, &st).await.map_err(|e| e.to_string())?;
            return Ok(());
        }
        let shared = inner.body.get("mid").and_then(Value::as_str).map(str::to_string);
        let gid_store = shared.as_deref().unwrap_or(&dedup_id);
        let appended = store_incoming_group_message(
            group_ctx,
            &c.did,
            text,
            inner.ts,
            Some(gid_store),
            atts,
            inner.body.get("sn").and_then(Value::as_str),
            parse_reply_ref(&inner.body),
        )
        .await?;
        write_ratchet(&c.did, &st).await.map_err(|e| e.to_string())?;
        if appended {
            maybe_rotate_inbound_queue(&c.did).await;
        }
        return Ok(());
    }

    // Verse lane: divert to the ephemeral inbox — the ratchet still advanced,
    // so persist the consumed state, but nothing reaches the conversation.
    if let Some(vp) = text.strip_prefix(VERSE_PREFIX) {
        // F-VERSE-BLOCK-BYPASS: drop a blocked sender's frame, but STILL persist the advanced
        // ratchet below so the session can't desync (the message was already consumed).
        if !is_blocked(&c.did).await {
            verse_push(&c.did, vp);
        }
        write_ratchet(&c.did, &st).await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    if text.is_empty() && atts.is_empty() {
        return Err("message body has neither text nor attachments".into());
    }

    // Store plaintext FIRST, persist the consumed advance LAST. Use the
    // sender's own message id (so both phones key it identically — the fix that
    // lets reactions/edits/deletes match on the receiver); fall back to the
    // envelope-derived id for older senders that don't carry "mid".
    let shared_id = inner.body.get("mid").and_then(Value::as_str).map(str::to_string);
    let store_id = shared_id.as_deref().unwrap_or(&dedup_id);
    // The sender's live nickname ("sn") is folded into the store's single contacts
    // RMW (coalesced) instead of a separate refresh_contact_name write.
    let sn = inner
        .body
        .get("sn")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let appended = store_incoming_message(&c.did, text, inner.ts, Some(store_id), atts, sn, parse_reply_ref(&inner.body)).await?;
    write_ratchet(&c.did, &st)
        .await
        .map_err(|e| e.to_string())?;
    if appended {
        maybe_rotate_inbound_queue(&c.did).await;
    }
    Ok(())
}

/// Handle a handshake reply that landed on one of OUR queues. The
/// queue id (NOT the sender_did) is the disambiguator — when we
/// minted the invite we didn't know who the recipient would be.
///
/// After promoting the contact to Active, we ROTATE: mint a fresh
/// Alice-side queue, send a `welcome` message on Bob's queue telling
/// him to switch to it, and retire the original invite queue
/// (peer_receiver::forget_topic + outbox::purge_topic). The original
/// invite queue is single-use from this moment on — even if the
/// invite link leaks to a third party, sending on it goes nowhere.
/// Complete the responder side of the ratchet bootstrap from a handshake's
/// `ratchet` block + the prekey we stashed at generate_invite. Returns Ok(true)
/// when a ratchet state was written, Ok(false) when there's nothing to
/// bootstrap (no stashed prekey), Err on a hard failure.
async fn bootstrap_responder_ratchet(
    c: &DmContact,
    placeholder_did: &str,
    rb: &Value,
    real_did: &str,
) -> Result<bool, String> {
    let Some(prekey_state) = read_ratchet(placeholder_did).await else {
        return Ok(false); // no stashed prekey — negotiate single-shot
    };
    if prekey_state.is_bootstrapped() {
        return Ok(false); // already a full state; nothing to do
    }
    let field = |k: &str| -> Result<String, String> {
        rb.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("ratchet block missing {k}"))
    };
    let eph_pub = B64
        .decode(field("eph_pub_b64")?)
        .map_err(|e| format!("ratchet eph b64: {e}"))?;
    let kem_ct = B64
        .decode(field("kem_ct_b64")?)
        .map_err(|e| format!("ratchet kem_ct b64: {e}"))?;
    let bob_dh: [u8; 32] = B64
        .decode(field("dh_pub_b64")?)
        .map_err(|e| format!("ratchet dh b64: {e}"))?
        .try_into()
        .map_err(|_| "ratchet dh wrong size".to_string())?;

    // The accepter's rolling KEM pub (optional — old accepters omit it, keeping
    // us classical). Present ⇒ our first sending chain goes hybrid.
    let peer_kem_pub = match rb.get("kem_pub_b64").and_then(|v| v.as_str()) {
        Some(s) => Some(
            B64.decode(s)
                .map_err(|e| format!("ratchet kem_pub b64: {e}"))?,
        ),
        None => None,
    };

    let via = decrypt_via_for_contact(c)?;
    let (x3dh, kem_ss) = shared_secrets(&via, &eph_pub, &kem_ct).await?;
    let sk = crypto::root_init(&x3dh, &kem_ss);
    let prekey_priv = b32(&prekey_state.dhs_priv)?;
    let prekey_pub = b32(&prekey_state.dhs_pub)?;
    let state = ratchet_init_responder(sk, prekey_priv, prekey_pub, bob_dh, peer_kem_pub)?;
    // Write the bootstrapped state under the peer's real DID, but DO NOT remove
    // the prekey stash here — the caller removes it only AFTER write_contacts
    // durably promotes the contact (so a contacts-write failure leaves the stash
    // intact and a redelivered handshake can re-bootstrap, rather than wedging
    // the contact in PendingInvite with its prekey already gone).
    write_ratchet(real_did, &state)
        .await
        .map_err(|e| e.to_string())?;
    Ok(true)
}

async fn receive_handshake(inner: &InnerPayload, on_queue: &str) -> Result<(), String> {
    let their_queue = inner
        .body
        .get("my_inbound_queue")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "handshake missing my_inbound_queue".to_string())?;
    let their_keys: PeerKeys = inner
        .body
        .get("pubkeys")
        .ok_or_else(|| "handshake missing pubkeys".to_string())
        .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| format!("pubkeys: {e}")))?;
    let their_name = inner
        .body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    // The accepter's node ticket — bootstrap the mesh to their queue below.
    let their_ticket = inner
        .body
        .get("node_ticket")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // Serialize the promote read-modify-write against the other receive-path RMW
    // (continuity-pin / welcome / queue-rotation / touch). Held for the function
    // body; handshakes are rare so this never contends in practice. NOTE: nothing
    // below calls another gated contact fn (no re-entrancy).
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let pos = list.iter().position(|c| {
        c.my_inbound_queue.as_deref() == Some(on_queue) && c.status == ContactStatus::PendingInvite
    });
    let Some(pos) = pos else {
        // Either a replayed handshake (sender retried on top of an
        // already-promoted contact) or a stranger guessed the queue
        // id (astronomically unlikely with 256 bits of entropy). Log
        // so the debug console shows what happened.
        crate::plat::warn(&format!(
            "[hey-core] handshake replay or stranger on queue {} from {}",
            on_queue, inner.sender_did
        ));
        return Ok(());
    };

    // Rotate: mint Alice's ongoing queue and pseudonyms; the old
    // invite queue retires below.
    let new_queue = random_hex(32);
    let new_recv_pseudonym = random_hex(16);

    let mut c = list.remove(pos);
    let old_queue = c.my_inbound_queue.clone();
    // Capture the pseudonym we READ the old invite queue with, BEFORE the rotation
    // below overwrites it — needed to keep reading the retired queue during grace.
    let old_recv_pseudonym = c.my_recv_pseudonym.clone();
    let placeholder_did = c.did.clone(); // "pending:<queue>" — keys the prekey stash
    c.did = inner.sender_did.clone();
    c.their_inbound_queue = Some(their_queue.to_string());
    c.peer_pubkeys = Some(their_keys.clone());
    // Cache the accepter's node ticket so our future sends to their queue
    // (including after either side rotates) re-dial the cross-runtime mesh.
    if !their_ticket.is_empty() {
        c.peer_ticket = Some(their_ticket.clone());
    }
    c.status = ContactStatus::Active;
    // F-09: first-contact keys are TOFU — we just learned this peer's real DID +
    // pubkeys from the handshake (sender_did was previously unknown), so they are
    // NOT yet verified out-of-band. Flag the promoted contact unverified; an
    // explicit OOB safety-number check upgrades it (the verified=true inherited
    // from the pending placeholder would have falsely claimed verification).
    c.key_verified = false;
    // F-HANDSHAKE-GATE: the promoted contact's keys are TOFU (key_verified=false above), and an
    // invite link is SHAREABLE — anyone who obtains it can craft this handshake with their OWN keys.
    // So the sensitive auto-shares (wallet address card on chat-open, call ticket — SOH control
    // messages EXEMPT from the 3921 text gate but self-checking needs_verify_before_send) must NOT
    // seal to these unverified-from-shared-link keys. Gate them exactly like the feed-follow path
    // (bootstrap_contact_from_keys: needs_verify_before_send = !verified). The user's first text
    // surfaces the verify/“send anyway” prompt; the welcome below rides a raw envelope (not the
    // gated send path), so handshake completion is unaffected.
    c.needs_verify_before_send = true;
    // A real handshake name beats a placeholder (empty / "pending:" / a
    // generated "hey-XXXXXX" label); never downgrade a real name to a placeholder.
    if is_generated_label(&c.name) && !is_generated_label(their_name) {
        c.name = their_name.into();
    }
    c.last_ts = inner.ts;
    c.last_preview = "Invite accepted ✓".into();
    c.my_inbound_queue = Some(new_queue.clone());
    c.my_recv_pseudonym = Some(new_recv_pseudonym);

    // F-ANON-INVITE-QUEUE-GRACE: retire the original invite queue into the GRACE
    // list instead of forgetting it immediately (the unconditional forget below
    // is dropped). This mirrors the continuous-rotation path (rotate_contact_queue)
    // so the inviter keeps LISTENING on the invite queue for QUEUE_GRACE_MS while
    // the welcome (announcing `new_queue`) is in flight to the accepter.
    //
    // Why this is anon-critical and regular-safe:
    //   • ANONYMOUS contacts have NO deterministic fallback — the accepter can't
    //     derive any queue without our real DID, so until it processes the welcome
    //     it keeps sending on the invite queue (send_body_to_contact uses
    //     c.their_inbound_queue, the minted invite queue). Forgetting it the instant
    //     we promote silently drops every such in-flight send, and if the welcome
    //     is lost the accepter is PERMANENTLY stranded on a dead queue.
    //   • REGULAR contacts already converge on the deterministic pair queue
    //     (my_v2_topics line ~7878 / send line ~4040), so this grace entry is
    //     harmless redundancy for them.
    // The grace entry is re-subscribed by my_v2_topics (retired_queues loop) and
    // routed home by owns_inbound_queue; prune_retired_queues drops it after grace.
    // No real-DID/identity material is involved — this is pure queue/topic sync, so
    // incognito privacy is unaffected.
    if let Some(old_q) = old_queue.clone() {
        c.retired_queues.push(RetiredQueue {
            queue: old_q,
            pseudonym: old_recv_pseudonym.clone().unwrap_or_else(|| "anonymous".into()),
            // LOCAL clock — the grace check in my_v2_topics compares against now_ms()
            // (a peer-supplied inner.ts could shrink/extend the window).
            retire_at: now_ms(),
        });
    }

    // Ratchet bootstrap (we are the RESPONDER), ATOMIC with promotion. The
    // accepter unilaterally committed ratchet_capable=true (its bootstrap is
    // purely local, can't fail), so if the handshake offers a ratchet we MUST
    // either bootstrap successfully OR refuse to promote — never go Active with
    // ratchet_capable=false while the peer is capable, which would brick the
    // conversation both ways with no recovery. On failure we return Err WITHOUT
    // writing contacts (the in-memory promotion is discarded) and WITHOUT
    // removing the prekey stash, so the contact stays PendingInvite and a
    // redelivered handshake retries once the provider recovers. Recovery uses
    // OUR key material via decrypt_via_for_contact — anon ⇒ local anon key,
    // provider-backed ⇒ runtime, else local seed (must-fix #3: anon never
    // touches the provider). A provider-down blip almost always fails the
    // handshake DECRYPT first (same provider ops), so this path is rare.
    let offered_ratchet = inner.body.get("ratchet").cloned();
    let ratchet_capable = if let Some(rb) = offered_ratchet {
        match bootstrap_responder_ratchet(&c, &placeholder_did, &rb, &inner.sender_did).await {
            Ok(true) => true,
            Ok(false) => {
                return Err(
                    "responder ratchet bootstrap: prekey stash missing — re-invite to re-establish"
                        .into(),
                );
            }
            Err(e) => {
                return Err(format!(
                    "responder ratchet bootstrap failed (handshake will retry once recovered): {e}"
                ));
            }
        }
    } else {
        // Peer didn't advertise ratchet — single-shot; drop the prekey stash.
        remove_ratchet(&placeholder_did).await;
        false
    };
    c.ratchet_capable = ratchet_capable;

    // Capture our identity for this contact before moving `c` into the list:
    // the welcome we send below must be signed as the SAME identity the peer
    // knows us by (real DID in Regular, ephemeral DID in Anonymous).
    let my_mode = c.mode;
    let my_anon = c.anon_identity.clone();
    // DUP-MERGE: this handshake promotes a PendingInvite to inner.sender_did. If an OTHER record
    // already exists for that real DID (mutual-invite, re-pair, or a follow-bootstrapped contact),
    // pushing `c` would create a DUPLICATE — and find_contact + inbound ratchet routing both pick
    // FIRST-match, so send and receive could resolve DIFFERENT records (wrong queue / key /
    // verify-gate, and the chat-list crash). Collapse to one: `c` (the fresh handshake) is
    // authoritative (its keys/queue/ratchet were just established); drop any pre-existing same-DID
    // record and carry over its retired queues so its rotated queues stay retired.
    let mut i = 0;
    while i < list.len() {
        if list[i].did == c.did {
            let old = list.remove(i);
            // A re-pair already surfaces as UNVERIFIED (handshake promotes via key_verified=false)
            // with wallet-card/call/feed-key auto-shares gated by needs_verify_before_send. We do
            // NOT key a "safety number changed" alarm on `old.key_verified`, because that flag is
            // also set provisionally by invite/signed-link bootstrap, so it false-fires on a normal
            // incognito re-scan. But if the OLD record was genuinely OOB-verified (oob_verified —
            // set ONLY by verify_contact) AND the keys actually CHANGED, that is the real
            // key-substitution case: raise key_changed so confirm_unverified_send HARD-refuses and
            // only an explicit out-of-band re-verify (verify_contact) can re-open sends.
            let keys_changed = old.peer_pubkeys.as_ref().map(|k| (&k.x25519_pub_b64, &k.ml_kem_pub_b64))
                != c.peer_pubkeys.as_ref().map(|k| (&k.x25519_pub_b64, &k.ml_kem_pub_b64));
            if old.oob_verified && keys_changed {
                c.key_changed = true;
            }
            c.retired_queues.extend(old.retired_queues);
        } else {
            i += 1;
        }
    }
    list.push(c);
    list.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    write_contacts(&list).await.map_err(|e| e.to_string())?;
    // CHAT-CAPABILITY: a completed handshake means an invite I generated was accepted → this is an
    // explicit chat pairing, so permit a private chat with them.
    enable_chat(&inner.sender_did).await;

    // Contact is now durably promoted — only NOW retire the prekey stash (the
    // bootstrap wrote the real-DID ratchet state but deliberately left the stash
    // so a write_contacts failure above would have left a re-bootstrappable
    // PendingInvite). On the no-ratchet path the stash was already dropped.
    if ratchet_capable {
        remove_ratchet(&placeholder_did).await;
    }

    // Send the welcome on BOB's queue so he learns Alice's new queue.
    let s = match session::current() {
        Some(s) => s,
        None => return Ok(()),
    };
    let me_real = inner_to_my_did().unwrap_or_default();
    let (welcome_did, welcome_seed) =
        match signing_identity(my_mode, my_anon.as_ref(), &me_real, &s.auth_key_hex) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
    let welcome_body = json!({ "my_inbound_queue": new_queue });
    if !welcome_did.is_empty() {
        if let Ok(welcome_inner) = build_inner(
            KIND_WELCOME,
            &welcome_body,
            &welcome_did,
            &welcome_seed,
            None,
        )
        .await
        {
            if let Ok(envelope) = encrypt_inner_for_peer(&welcome_inner, &their_keys) {
                let wire = json!({
                    "type": "dm.v2",
                    "envelope": envelope,
                })
                .to_string();
                let bob_topic = format!("{TOPIC_PREFIX_V2}/{their_queue}");
                let send_pseudonym = random_hex(16);
                // Bootstrap the mesh to the accepter's runtime via their ticket.
                let boot: Vec<String> =
                    (!their_ticket.is_empty()).then(|| their_ticket.clone()).into_iter().collect();
                let _ = peer::join_topic_with(&bob_topic, &boot).await;
                let _ = crate::api::outbox::publish_or_enqueue(
                    &bob_topic,
                    &boot,
                    &send_pseudonym,
                    &wire,
                )
                .await;
            }
        }
    }

    // F-ANON-INVITE-QUEUE-GRACE: the original invite queue is NO LONGER forgotten
    // here. It was pushed into `retired_queues` above so we KEEP listening on it for
    // QUEUE_GRACE_MS — the accepter (especially in Anonymous mode, which has no
    // deterministic fallback queue) goes on sending to it until it processes our
    // welcome. The normal grace teardown (prune_retired_queues, fired from
    // maybe_rotate_inbound_queue) forget_topic + purge_topic's it once the window
    // lapses. Forgetting it here re-opened the silent-drop window the grace exists
    // to close, so the unconditional forget/purge was removed.

    Ok(())
}

/// Process a `welcome` payload: Bob learns Alice's rotated queue and
/// updates `their_inbound_queue` so his next send lands on the right
/// destination. Outbox items still pointing at Alice's old queue are
/// dropped — Alice isn't listening there anymore.
async fn receive_welcome(inner: &InnerPayload) -> Result<(), String> {
    let new_queue = inner
        .body
        .get("my_inbound_queue")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "welcome missing my_inbound_queue".to_string())?;
    // Serialize this contact RMW against the rest of the receive path (race fix).
    let _g = contacts_gate().lock().await;
    let mut list = list_contacts().await;
    let Some(c) = list.iter_mut().find(|c| c.did == inner.sender_did) else {
        crate::plat::warn(&format!(
            "[hey-core] welcome from unknown {}",
            inner.sender_did
        ));
        return Ok(());
    };
    let prev = c.their_inbound_queue.clone();
    c.their_inbound_queue = Some(new_queue.to_string());
    write_contacts(&list).await.map_err(|e| e.to_string())?;
    if let Some(prev) = prev {
        if prev != new_queue {
            let stale_topic = format!("{TOPIC_PREFIX_V2}/{prev}");
            crate::api::outbox::purge_topic(&stale_topic).await;
        }
    }
    Ok(())
}

/// The signed-in user's real DID — the runtime-projected `did:key` on the
/// session (wallet model; there is no local seed to derive it from). Returns
/// None if signed out or the session carries no valid did:key.
fn inner_to_my_did() -> Option<String> {
    session::current()
        .map(|s| s.did_key)
        .filter(|d| d.starts_with("did:key:z"))
}

// ── Self-test: v2 wire-format crypto roundtrip ───────────────────────
//
// Builds an inner payload signed with the current session's key,
// encrypts it to our own pubkeys, serializes the wire envelope,
// parses it back, decrypts, verifies the inner sig, and confirms the
// recovered payload matches. Also exercises the invite-link codec
// round-trip. Returns Ok("✓ …") or Err describing the failure step.
//
// This catches: bad JSON encoding of InnerPayload, broken hybrid PQ
// keys in the current session, sig-verify regressions, and invite-
// link base64url/JSON drift. It does NOT exercise the runtime peer
// provider — for that you need two real instances.

pub async fn self_test_v2() -> Result<String, String> {
    let me = ensure_profile()
        .await
        .map_err(|e| format!("profile: {e}"))?;
    let s = session::current().ok_or_else(|| "not signed in".to_string())?;
    let my_pub = my_pubkeys().await.ok_or_else(|| "no pubkeys".to_string())?;

    let body = json!({ "text": "self-test ping" });
    let inner = build_inner(KIND_MESSAGE, &body, &me.did_key, &s.auth_key_hex, None)
        .await
        .map_err(|e| format!("build_inner: {e}"))?;

    let envelope = encrypt_inner_for_peer(&inner, &my_pub).map_err(|e| format!("encrypt: {e}"))?;
    let wire = json!({
        "type": "dm.v2",
        "envelope": envelope,
    })
    .to_string();

    let v: Value = serde_json::from_str(&wire).map_err(|e| format!("wire reparse: {e}"))?;
    if v.get("type").and_then(|t| t.as_str()) != Some("dm.v2") {
        return Err("type field missing on reparse".into());
    }
    let env_val = v
        .get("envelope")
        .ok_or_else(|| "no envelope on reparse".to_string())?;
    let env_back: HpqEnvelope =
        serde_json::from_value(env_val.clone()).map_err(|e| format!("envelope reparse: {e}"))?;
    let inner_back = decrypt_envelope_to_inner(&env_back, &DecryptVia::Provider)
        .await
        .map_err(|e| format!("decrypt: {e}"))?;
    if !verify_inner(&inner_back) {
        return Err("inner signature did NOT verify".into());
    }
    if inner_back.sender_did != me.did_key {
        return Err(format!(
            "sender_did mismatch: got {} expected {}",
            inner_back.sender_did, me.did_key
        ));
    }
    let recovered = inner_back
        .body
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if recovered != "self-test ping" {
        return Err(format!("text mismatch: got {recovered:?}"));
    }

    // (Anonymous-mode round-trip self-test removed with incognito.)

    // Invite-link codec roundtrip — independent of envelope crypto.
    let mut invite = InviteLink {
        v: INVITE_LINK_VERSION,
        queue: random_hex(32),
        did: me.did_key.clone(),
        name: "self-test".into(),
        keys: my_pub,
        nonce: random_hex(16),
        expires_at: now_ms() + INVITE_TTL_MS,
        ratchet: None,
        node_ticket: None,
        sig: None,
    };
    sign_invite(&mut invite, &seed32(&s.auth_key_hex)?);
    let encoded = format!("hey-invite:{}", encode_invite_link(&invite));
    let decoded = decode_invite_link(&encoded).map_err(|e| format!("invite decode: {e}"))?;
    if decoded.did != invite.did || decoded.queue != invite.queue || decoded.nonce != invite.nonce {
        return Err("invite link round-trip mismatch".into());
    }
    if decoded.expires_at != invite.expires_at {
        return Err("invite expires_at mismatch".into());
    }
    if !crate::api::outbox::schema_roundtrip_ok() {
        return Err("outbox schema roundtrip broken".into());
    }

    // Chain the pure ratchet + queue-rotation self-tests so this one debug entry
    // point exercises the hybrid PQ ratchet and continuous rotation too.
    let ratchet = self_test_ratchet().map_err(|e| format!("ratchet self-test: {e}"))?;
    let rotation =
        self_test_queue_rotation().map_err(|e| format!("queue-rotation self-test: {e}"))?;

    Ok(format!(
        "✓ v2 envelope + anon round-trip + invite codec + outbox schema OK\n{ratchet}\n{rotation}"
    ))
}

// ── Double Ratchet self-test (pure, no storage/provider) ─────────────
//
// Drives two in-memory ratchet states through bootstrap + a multi-message
// exchange and asserts the must-fix failure modes. Touches no session, storage,
// or identity provider — but it is NOT host-pure: the state machine stamps
// skipped-key timestamps via js_sys::Date::now, so run it from a wasm debug
// console like self_test_v2 (not native `cargo test`). A wasm_bindgen_test
// wrapper to gate it in CI is a TODO.

/// Seal `text` as a ratchet message from `st` to a recipient whose STATIC
/// ML-KEM public key is `recip_kem_pub`. Returns the full ratchet header (page
/// number + rolling-KEM fields) + env.
fn rt_send(
    st: &mut RatchetState,
    recip_kem_pub: &[u8],
    text: &str,
) -> Result<(RatchetHeader, HpqEnvelope), String> {
    let (mk, header) = ratchet_step_send(st)?;
    // Wipe the transient message key on scope exit (L: transient AEAD key not
    // zeroized). Derefs to `[u8;32]`, so `&mk` into encrypt_with_mk is unchanged.
    let mk = zeroize::Zeroizing::new(mk);
    let dhs_pub: [u8; 32] = B64
        .decode(&header.dh)
        .map_err(|e| format!("dh b64: {e}"))?
        .try_into()
        .map_err(|_| "dh size".to_string())?;
    let env = crypto::encrypt_with_mk(text, &mk, recip_kem_pub, &dhs_pub)?;
    Ok((header, env))
}

/// Open a ratchet envelope into `st` (copy-on-write: `st` is committed ONLY on
/// a successful authenticated decrypt), using our static ML-KEM secret. The
/// `header` supplies the page number AND the rolling-KEM ct/pub (as they'd ride
/// cleartext on the wire).
fn rt_recv(
    st: &mut RatchetState,
    env: &HpqEnvelope,
    header: &RatchetHeader,
    our_kem_secret: &[u8],
) -> Result<String, String> {
    let dh_bytes: [u8; 32] = B64
        .decode(&env.eph)
        .map_err(|e| format!("eph b64: {e}"))?
        .try_into()
        .map_err(|_| "eph size".to_string())?;
    let dh_hex = hx(&dh_bytes);
    let mut clone = st.clone();
    let mk = ratchet_step_recv(
        &mut clone,
        &dh_hex,
        dh_bytes,
        header.pn,
        header.n,
        header.kem_ct.as_deref(),
        header.kem_pub.as_deref(),
    )?;
    // Wipe the transient message key on scope exit (L: transient AEAD key not
    // zeroized). Derefs to `[u8;32]`, so `&mk` into open_with_secrets is unchanged.
    let mk = zeroize::Zeroizing::new(mk);
    let kem_ct = B64.decode(&env.kem).map_err(|e| format!("kem b64: {e}"))?;
    let kem_ss = crypto::ml_kem_decapsulate_local(&kem_ct, our_kem_secret)?;
    let pt = crypto::open_with_secrets(env, &*mk, &kem_ss)?;
    *st = clone; // commit
    Ok(pt)
}

pub fn self_test_ratchet() -> Result<String, String> {
    // Static identity material for A (inviter/responder) and B (accepter/initiator).
    let (a_x_priv, a_x_pub) = crypto::ratchet_keypair();
    let (a_kem_secret, a_kem_pub) = crypto::generate_ml_kem_keypair();
    let (_b_x_priv, _b_x_pub) = crypto::ratchet_keypair();
    let (b_kem_secret, b_kem_pub) = crypto::generate_ml_kem_keypair();
    // A's published ratchet prekey.
    let (a_rk_priv, a_rk_pub) = crypto::ratchet_keypair();

    // ── Bootstrap. B (initiator) derives SK from a fresh bootstrap ephemeral. ──
    let (b_eph_priv, b_eph_pub) = crypto::ratchet_keypair();
    let x3dh_b = crypto::dh(&b_eph_priv, &a_x_pub);
    let (kem_ct, kem_ss_b) = crypto::ml_kem_encapsulate_local(&a_kem_pub)?;
    let sk_b = crypto::root_init(&x3dh_b, &kem_ss_b);
    let mut state_b = ratchet_init_initiator(sk_b, a_rk_pub);
    let b_dh_pub: [u8; 32] = b32(&state_b.dhs_pub)?;

    // A (responder) recovers SK from its static key + B's bootstrap ephemeral.
    let x3dh_a = crypto::dh(&a_x_priv, &b_eph_pub);
    let kem_ss_a = crypto::ml_kem_decapsulate_local(&kem_ct, &a_kem_secret)?;
    let sk_a = crypto::root_init(&x3dh_a, &kem_ss_a);
    if sk_a != sk_b {
        return Err("bootstrap SK mismatch between initiator and responder".into());
    }
    // Hybrid: A (responder) bootstraps with B's rolling KEM pub, so A's first
    // sending chain encapsulates to it (the inviter→accepter direction is hybrid
    // from message one). B's first chain is classical until B's first turn.
    let b_roll_kem = B64
        .decode(
            state_b
                .kem_pub
                .as_ref()
                .ok_or_else(|| "initiator missing rolling KEM pub".to_string())?,
        )
        .map_err(|e| format!("b rolling kem b64: {e}"))?;
    let mut state_a =
        ratchet_init_responder(sk_a, a_rk_priv, a_rk_pub, b_dh_pub, Some(b_roll_kem))?;

    // ── 4-message exchange B→A→B→A, forcing DH turns both ways. ──
    let b_dhs0 = state_b.dhs_priv.clone();
    let b_kem_priv0 = state_b.kem_priv.clone();
    let (h, env) = rt_send(&mut state_b, &a_kem_pub, "m1")?;
    if rt_recv(&mut state_a, &env, &h, &a_kem_secret)? != "m1" {
        return Err("m1 round-trip failed".into());
    }
    let (h, env) = rt_send(&mut state_a, &b_kem_pub, "m2")?;
    if rt_recv(&mut state_b, &env, &h, &b_kem_secret)? != "m2" {
        return Err("m2 round-trip failed".into());
    }
    // Receiving m2 must have turned B's DH ratchet — the sending key rotated...
    if state_b.dhs_priv == b_dhs0 {
        return Err("dhs_priv was REUSED across a DH turn (must-fix #5)".into());
    }
    // ...and so must B's ROLLING KEM private (PQ-PCS: the old KEM private, whose
    // compromise would let a quantum attacker recover the chain, is discarded).
    if state_b.kem_priv == b_kem_priv0 {
        return Err("kem_priv was REUSED across a DH turn (PQ-PCS not delivered)".into());
    }
    let (h3, env3) = rt_send(&mut state_b, &a_kem_pub, "m3")?;
    let (h4, env4) = rt_send(&mut state_a, &b_kem_pub, "m4")?;
    if rt_recv(&mut state_a, &env3, &h3, &a_kem_secret)? != "m3" {
        return Err("m3 round-trip failed (forced DH turn)".into());
    }
    if rt_recv(&mut state_b, &env4, &h4, &b_kem_secret)? != "m4" {
        return Err("m4 round-trip failed".into());
    }
    // By now both directions are fully hybrid — every turn carries a kem_ct.
    if h3.kem_ct.is_none() || h4.kem_ct.is_none() {
        return Err("post-bootstrap turns are not carrying rolling KEM ct".into());
    }

    // ── Out-of-order within a chain (≤ MAX_SKIP), across a fresh DH epoch. ──
    let (h5, env5) = rt_send(&mut state_b, &a_kem_pub, "m5")?;
    let (h6, env6) = rt_send(&mut state_b, &a_kem_pub, "m6")?;
    let (h7, env7) = rt_send(&mut state_b, &a_kem_pub, "m7")?;
    if rt_recv(&mut state_a, &env7, &h7, &a_kem_secret)? != "m7" {
        return Err("out-of-order m7 (head) failed".into());
    }
    if rt_recv(&mut state_a, &env5, &h5, &a_kem_secret)? != "m5" {
        return Err("out-of-order m5 (from skipped) failed".into());
    }
    if rt_recv(&mut state_a, &env6, &h6, &a_kem_secret)? != "m6" {
        return Err("out-of-order m6 (from skipped) failed".into());
    }

    // ── Replay of a consumed message must NOT decrypt (old mk deleted). ──
    if rt_recv(&mut state_a, &env7, &h7, &a_kem_secret).is_ok() {
        return Err("replay of a consumed message decrypted (mk not deleted)".into());
    }

    // ── Skip caps rejected BEFORE any KDF (must-fix #7) — same-epoch AND the
    //    cross-epoch double-skip (the case the per-call cap used to miss). The
    //    combined-skip cap runs BEFORE the turn's KEM step, so None kem args are
    //    fine (the cap rejects first). ──
    {
        // Same-epoch: n beyond nr + MAX_SKIP.
        let mut probe = state_a.clone();
        let dhr = state_a
            .dhr_pub
            .clone()
            .ok_or_else(|| "no dhr for cap probe".to_string())?;
        let dh_bytes = b32(&dhr)?;
        let huge = state_a.nr.saturating_add(MAX_SKIP).saturating_add(5);
        if ratchet_step_recv(&mut probe, &dhr, dh_bytes, 0, huge, None, None).is_ok() {
            return Err("same-epoch skip beyond MAX_SKIP was not rejected".into());
        }
        // Cross-epoch: a forged FRESH eph with old-chain pn + new-chain n whose
        // COMBINED work exceeds MAX_SKIP must be rejected (else 2*MAX_SKIP KDFs).
        let mut probe2 = state_a.clone();
        let (_, fake_dh) = crypto::ratchet_keypair();
        let fake_hex = hx(&fake_dh);
        let pn_big = state_a.nr.saturating_add(MAX_SKIP - 100); // old-chain skip ~MAX_SKIP-100
        let n_big = 200; // new-chain skip 200 ⇒ combined > MAX_SKIP
        if ratchet_step_recv(&mut probe2, &fake_hex, fake_dh, pn_big, n_big, None, None).is_ok() {
            return Err("cross-epoch combined skip beyond MAX_SKIP was not rejected".into());
        }
    }

    // ── Tampered page number / swapped DH must fail (AEAD authenticates). ──
    let (h_t, env_t) = rt_send(&mut state_b, &a_kem_pub, "tamper")?;
    let mut h_badn = h_t.clone();
    h_badn.n = h_badn.n.wrapping_add(1);
    if rt_recv(&mut state_a.clone(), &env_t, &h_badn, &a_kem_secret).is_ok() {
        return Err("tampered page number decrypted".into());
    }
    let mut env_swapped = env_t.clone();
    let (_, fake_pub) = crypto::ratchet_keypair();
    env_swapped.eph = B64.encode(fake_pub);
    if rt_recv(&mut state_a.clone(), &env_swapped, &h_t, &a_kem_secret).is_ok() {
        return Err("swapped ratchet DH decrypted".into());
    }
    // The untampered original still opens (the failed attempts used clones).
    if rt_recv(&mut state_a, &env_t, &h_t, &a_kem_secret)? != "tamper" {
        return Err("untampered message failed after tamper attempts".into());
    }

    // ── Tampered rolling KEM ct on a TURN must fail (PQ-PCS is bound). Force a
    //    fresh B turn (A sends → B turns on its next send), then corrupt the
    //    turn's kem_ct: ML-KEM implicit-rejection yields a DIFFERENT secret →
    //    wrong receiving chain key → AEAD fails. ──
    let (h_setup, env_setup) = rt_send(&mut state_a, &b_kem_pub, "turn-setup")?;
    if rt_recv(&mut state_b, &env_setup, &h_setup, &b_kem_secret)? != "turn-setup" {
        return Err("turn-setup A→B failed".into());
    }
    let (h_turn, env_turn) = rt_send(&mut state_b, &a_kem_pub, "kem-turn")?;
    let ct_b64 = h_turn
        .kem_ct
        .clone()
        .ok_or_else(|| "expected a rolling kem_ct on a post-bootstrap turn".to_string())?;
    let mut bad_ct = B64
        .decode(&ct_b64)
        .map_err(|e| format!("kem_ct b64: {e}"))?;
    bad_ct[0] ^= 0x01;
    let mut h_badkem = h_turn.clone();
    h_badkem.kem_ct = Some(B64.encode(&bad_ct));
    if rt_recv(&mut state_a.clone(), &env_turn, &h_badkem, &a_kem_secret).is_ok() {
        return Err("tampered rolling kem_ct still decrypted (hybrid PCS not bound)".into());
    }
    if rt_recv(&mut state_a, &env_turn, &h_turn, &a_kem_secret)? != "kem-turn" {
        return Err("untampered kem-turn failed after tamper attempt".into());
    }

    // (Anonymous-contact decrypt-path self-test removed with incognito.)

    // ── Classical fallback: two PRE-UPGRADE contacts (no rolling KEM) still
    //    round-trip. Bootstrap then strip the rolling-KEM fields from both,
    //    simulating a ratchet established before the hybrid upgrade. ──
    {
        let (fa_x_priv, fa_x_pub) = crypto::ratchet_keypair();
        let (fa_kem_secret, fa_kem_pub) = crypto::generate_ml_kem_keypair();
        let (fa_rk_priv, fa_rk_pub) = crypto::ratchet_keypair();
        let (fb_eph_priv, fb_eph_pub) = crypto::ratchet_keypair();
        let fx3dh_b = crypto::dh(&fb_eph_priv, &fa_x_pub);
        let (fkem_ct, fkem_ss_b) = crypto::ml_kem_encapsulate_local(&fa_kem_pub)?;
        let fsk = crypto::root_init(&fx3dh_b, &fkem_ss_b);
        let mut fstate_b = ratchet_init_initiator(fsk, fa_rk_pub);
        let fb_dh: [u8; 32] = b32(&fstate_b.dhs_pub)?;
        let fx3dh_a = crypto::dh(&fa_x_priv, &fb_eph_pub);
        let fkem_ss_a = crypto::ml_kem_decapsulate_local(&fkem_ct, &fa_kem_secret)?;
        let fsk_a = crypto::root_init(&fx3dh_a, &fkem_ss_a);
        // No peer rolling KEM ⇒ a classical responder bootstrap.
        let mut fstate_a = ratchet_init_responder(fsk_a, fa_rk_priv, fa_rk_pub, fb_dh, None)?;
        for st in [&mut fstate_a, &mut fstate_b] {
            st.kem_priv = None;
            st.kem_pub = None;
            st.peer_kem_pub = None;
            st.send_kem_ct = None;
        }
        let (fh, fenv) = rt_send(&mut fstate_b, &fa_kem_pub, "classical")?;
        if fh.kem_ct.is_some() || fh.kem_pub.is_some() {
            return Err("classical contact emitted rolling KEM wire fields".into());
        }
        if rt_recv(&mut fstate_a, &fenv, &fh, &fa_kem_secret)? != "classical" {
            return Err("classical fallback round-trip failed".into());
        }
    }

    Ok("✓ hybrid ratchet bootstrap + 4-msg DH turns (KEM private rotates) + out-of-order + replay/tamper/cap/kem-ct rejects + classical fallback + anon-local OK".into())
}

/// Pure self-test for continuous queue rotation (no storage/session/provider):
/// rotate installs a fresh queue + stashes the old one in the grace list, and
/// the grace prune drops it only after QUEUE_GRACE_MS.
pub fn self_test_queue_rotation() -> Result<String, String> {
    let mut c = DmContact {
        did: "did:key:zSelfTest".into(),
        peer_ticket: None,
        ticket_self_asserted: false,
        name: "t".into(),
        last_ts: 0,
        last_preview: String::new(),
        unread: 0,
        my_inbound_queue: Some(random_hex(32)),
        my_recv_pseudonym: Some(random_hex(16)),
        their_inbound_queue: Some(random_hex(32)),
        my_send_pseudonym: Some(random_hex(16)),
        peer_pubkeys: None,
        key_pop: None,
        status: ContactStatus::Active,
        mode: IdentityMode::Regular,
        anon_identity: None,
        ratchet_capable: false,
        key_verified: true,
        key_changed: false,
        oob_verified: false,
        my_queue_rotated_at: 1_000,
        my_queue_msg_count: 5,
        retired_queues: Vec::new(),
        salted_queue: None,
        peer_salted: false,
        peer_salted_at: 0,
        salted_self_ready_at: 0,
        needs_verify_before_send: false,
    };
    let old_queue = c.my_inbound_queue.clone().unwrap();
    let now = 2_000;
    let new_queue = rotate_contact_queue(&mut c, now);
    if c.my_inbound_queue.as_deref() != Some(new_queue.as_str()) {
        return Err("rotate did not install the new queue".into());
    }
    if new_queue == old_queue {
        return Err("rotate reused the same queue id".into());
    }
    if c.my_queue_msg_count != 0 || c.my_queue_rotated_at != now {
        return Err("rotate did not reset the clock/counter".into());
    }
    if !c
        .retired_queues
        .iter()
        .any(|r| r.queue == old_queue && r.retire_at == now)
    {
        return Err("rotate did not stash the old queue in the grace list".into());
    }
    // Inside the grace window: prune keeps it (so my_v2_topics keeps polling it).
    let expired = prune_retired_queues(&mut c, now + QUEUE_GRACE_MS - 1);
    if !expired.is_empty() || c.retired_queues.len() != 1 {
        return Err("prune dropped a queue still inside the grace window".into());
    }
    // Past the grace window: prune drops it and returns its topic to forget.
    let expired = prune_retired_queues(&mut c, now + QUEUE_GRACE_MS);
    if expired.len() != 1 || !expired[0].ends_with(&old_queue) || !c.retired_queues.is_empty() {
        return Err("prune did not retire an expired grace queue".into());
    }
    Ok("✓ queue rotation: install + grace stash + prune at TTL OK".into())
}

// ── Identity wipe ────────────────────────────────────────────────────
//
// Counterpart to session::wipe_identity. Drops every DM artifact:
// contacts list, peer-keys cache, every per-DID conversation file, the
// expiry map, and the outbox. Iterates the contact list FIRST so we
// know which conversation files to delete (storage doesn't expose a
// directory listing).

pub async fn wipe_dm_storage() {
    let contacts = list_contacts().await;
    for c in &contacts {
        let _ = storage::remove(&conv_path(&c.did)).await;
        // Per-contact ratchet state + any not-yet-completed prekey stash.
        remove_ratchet(&c.did).await;
        if let Some(q) = &c.my_inbound_queue {
            remove_ratchet(&format!("pending:{q}")).await;
        }
    }
    let _ = storage::remove(CONTACTS_FILE).await;
    let _ = storage::remove(PEER_KEYS_FILE).await;
    let _ = storage::remove(EXPIRY_FILE).await;
    crate::api::outbox::clear().await;
}

// ── Continuous queue rotation ────────────────────────────────────────

/// Pure in-memory rotation: stash the current inbound queue in the grace list,
/// mint + install a fresh one, reset the rotation clock + counter. Returns the
/// new queue id (the caller joins its topic + announces it via a `welcome`).
/// No I/O — unit-testable by self_test_queue_rotation.
fn rotate_contact_queue(c: &mut DmContact, now: i64) -> String {
    let new_queue = random_hex(32);
    let new_pseudonym = random_hex(16);
    if let Some(old_q) = c.my_inbound_queue.take() {
        let old_p = c
            .my_recv_pseudonym
            .take()
            .unwrap_or_else(|| "anonymous".into());
        c.retired_queues.push(RetiredQueue {
            queue: old_q,
            pseudonym: old_p,
            retire_at: now,
        });
    }
    c.my_inbound_queue = Some(new_queue.clone());
    c.my_recv_pseudonym = Some(new_pseudonym);
    c.my_queue_rotated_at = now;
    c.my_queue_msg_count = 0;
    new_queue
}

/// Drop retired queues past the grace window; returns their `q/<id>` topics so
/// the caller can forget_topic + purge their outbox. No I/O.
fn prune_retired_queues(c: &mut DmContact, now: i64) -> Vec<String> {
    let mut expired = Vec::new();
    c.retired_queues.retain(|rq| {
        if now - rq.retire_at >= QUEUE_GRACE_MS {
            expired.push(format!("{TOPIC_PREFIX_V2}/{}", rq.queue));
            false
        } else {
            true
        }
    });
    expired
}

/// Send a `welcome` telling `their_queue` to switch its sends to `new_queue`.
/// Signed as the identity this contact knows us by (Regular real DID / Anonymous
/// ephemeral). Best-effort: any failure just means the peer keeps using the old
/// queue, which we still poll during the grace window — so no message is lost.
async fn send_rotation_welcome(
    their_queue: &str,
    their_keys: &PeerKeys,
    new_queue: &str,
    mode: IdentityMode,
    anon: Option<&AnonIdentity>,
    peer_ticket: Option<&str>,
) {
    let Some(s) = session::current() else {
        return;
    };
    let me_real = inner_to_my_did().unwrap_or_default();
    let (welcome_did, welcome_seed) = match signing_identity(mode, anon, &me_real, &s.auth_key_hex)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    if welcome_did.is_empty() {
        return;
    }
    let welcome_body = json!({ "my_inbound_queue": new_queue });
    let Ok(welcome_inner) = build_inner(
        KIND_WELCOME,
        &welcome_body,
        &welcome_did,
        &welcome_seed,
        None,
    )
    .await
    else {
        return;
    };
    let Ok(envelope) = encrypt_inner_for_peer(&welcome_inner, their_keys) else {
        return;
    };
    let wire = json!({ "type": "dm.v2", "envelope": envelope }).to_string();
    let topic = format!("{TOPIC_PREFIX_V2}/{their_queue}");
    // Bootstrap the mesh to the peer's runtime via their ticket (if known) so
    // the rotation welcome forms a neighbor and actually crosses runtimes —
    // symmetric with send_message_inner / receive_handshake. Bootstrap-less
    // here would send into an empty active_view and the peer would never learn
    // the new queue, stalling delivery once the old queue's grace window lapses.
    let boot: Vec<String> = peer_ticket
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .into_iter()
        .collect();
    let _ = peer::join_topic_with(&topic, &boot).await;
    let send_pseudonym = random_hex(16);
    let _ = crate::api::outbox::publish_or_enqueue(&topic, &boot, &send_pseudonym, &wire).await;
}

/// Count one inbound message against the contact's current queue and rotate it
/// if the trigger tripped (≥ N messages OR ≥ T elapsed, floored to once/hour).
/// The old queue moves to the grace list (still polled) instead of being dropped
/// immediately — so a peer mid-switch loses nothing. Best-effort; never blocks
/// delivery (the message is already stored before this runs).
async fn maybe_rotate_inbound_queue(peer_did: &str) {
    let now = now_ms();
    // RMW under the contacts gate (race fix); the network sends below run AFTER the
    // gate is released so we never hold the lock across I/O. `expired`/`announce`
    // are computed inside and consumed outside.
    let (expired, announce): (Vec<String>, Option<_>) = {
        let _g = contacts_gate().lock().await;
        let mut list = list_contacts().await;
        let Some(idx) = list
            .iter()
            .position(|c| c.did == peer_did && c.status == ContactStatus::Active)
        else {
            return;
        };
        if list[idx].my_inbound_queue.is_none() {
            return;
        }

        list[idx].my_queue_msg_count = list[idx].my_queue_msg_count.saturating_add(1);
        // Start the clock for a contact that never rotated (don't fire instantly).
        if list[idx].my_queue_rotated_at == 0 {
            list[idx].my_queue_rotated_at = now;
        }
        let expired = prune_retired_queues(&mut list[idx], now);

        let since = now - list[idx].my_queue_rotated_at;
        let due = since >= QUEUE_ROTATE_FLOOR_MS
            && (list[idx].my_queue_msg_count >= QUEUE_ROTATE_MSGS || since >= QUEUE_ROTATE_MS);

        let announce = if due {
            let new_queue = rotate_contact_queue(&mut list[idx], now);
            Some((
                new_queue,
                list[idx].their_inbound_queue.clone(),
                list[idx].peer_pubkeys.clone(),
                list[idx].mode,
                list[idx].anon_identity.clone(),
                list[idx].peer_ticket.clone(),
            ))
        } else {
            None
        };

        // One write covers the count bump, the prune, and the rotation.
        if write_contacts(&list).await.is_err() {
            return;
        }
        (expired, announce)
    };
    for t in &expired {
        crate::peer_receiver::forget_topic(t).await;
        crate::api::outbox::purge_topic(t).await;
    }
    if let Some((new_queue, their_queue, their_keys, mode, anon, peer_ticket)) = announce {
        let _ = peer::join_topic(&format!("{TOPIC_PREFIX_V2}/{new_queue}")).await;
        if let (Some(tq), Some(tk)) = (their_queue, their_keys) {
            send_rotation_welcome(&tq, &tk, &new_queue, mode, anon.as_ref(), peer_ticket.as_deref())
                .await;
        }
    }
}

// ── Helpers exposed to peer_receiver for subscription bookkeeping ────

/// Iterate v2 contacts and return the list of `hey-v0/q/<id>` topics we must
/// keep joined to receive their messages — the current inbound queue PLUS any
/// retired queues still inside the grace window (so a peer mid-rotation isn't
/// dropped).
/// Topics this runtime RECEIVES DMs on, as `(topic, consumer_id, bootstrap)`.
/// `bootstrap` is the contact's peer node ticket (if known) so the poll loop
/// joins our own inbound queue WITH the peer as a graft target — that forms the
/// symmetric topic neighbor proactively instead of waiting for the sender to
/// dial in. Without it the receiver joins empty-bootstrap and the neighbor only
/// ever forms when the OTHER side happens to send first.
pub async fn my_v2_topics() -> Vec<(String, String, Vec<String>)> {
    let now = now_ms();
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    let mut out = Vec::new();
    for c in list_contacts().await {
        let boot: Vec<String> = c.peer_ticket.iter().cloned().collect();
        let consumer_id = c
            .my_recv_pseudonym
            .clone()
            .unwrap_or_else(|| "anonymous".into());
        // Regular-mode contacts ALSO listen on the DETERMINISTIC per-pair queue
        // the sender now derives — both sides converge on the same q/<id> with
        // no handshake dependency (the cross-runtime DM fix). The minted queue
        // below is kept for in-flight / anon / legacy contacts.
        if matches!(c.mode, IdentityMode::Regular) && !my_did.is_empty() {
            // F-11: listen on the salted per-pair topic (derive + pin on first use).
            let salted = ensure_salted_queue(&c.did).await;
            if let Some(s) = salted.as_ref() {
                out.push((format!("{TOPIC_PREFIX_V2}/{s}"), consumer_id.clone(), boot.clone()));
            }
            // F-LEGACY-PAIR-TOPIC (re-fix): the legacy deterministic pair topic is
            // DID-derivable (a metadata leak). Keep subscribing it ONLY while the
            // migration to the salted topic isn't fully settled. The grace is now
            // driven by a SELF-owned event — the moment WE derived/pinned our own
            // salted topic (`salted_self_ready_at`) — NOT by the peer advertising
            // `sc:true`. A non-cooperating peer that never sends `sc:true` can no
            // longer keep us subscribed to the leaky legacy topic forever: once we
            // F-11 FIX: only abandon the legacy inbound subscription once the PEER has actually
            // migrated its SENDS (`peer_salted`), AND we can derive the salted topic, AND the
            // bounded grace has elapsed. The old gate dropped legacy on a self-only 24h timer
            // regardless of the peer — but the SEND side only switches to salted after it receives
            // OUR `sc:true`, which we only emit on a *delivered* message. If delivery ever flapped,
            // both ends stay peer_salted=false (still SENDING legacy) yet both timed out their
            // legacy LISTEN → permanent mutual silent drop. Requiring `peer_salted` guarantees we
            // never stop listening on a topic the peer is still sending on. (Cost: a bounded
            // metadata-leak window for a non-migrating peer — a privacy nicety, never a delivery
            // requirement; the salted topic above stays joined unconditionally.)
            let migrated_off_legacy = salted.is_some()
                && c.peer_salted
                && c.salted_self_ready_at != 0
                && now - c.salted_self_ready_at > LEGACY_TOPIC_GRACE_MS;
            if !migrated_off_legacy {
                let det = pair_inbound_queue(&my_did, &c.did);
                out.push((format!("{TOPIC_PREFIX_V2}/{det}"), consumer_id.clone(), boot.clone()));
            }
        }
        if let Some(q) = &c.my_inbound_queue {
            out.push((format!("{TOPIC_PREFIX_V2}/{q}"), consumer_id, boot.clone()));
        }
        for rq in &c.retired_queues {
            if now - rq.retire_at < QUEUE_GRACE_MS {
                let consumer_id = if rq.pseudonym.is_empty() {
                    "anonymous".into()
                } else {
                    rq.pseudonym.clone()
                };
                out.push((format!("{TOPIC_PREFIX_V2}/{}", rq.queue), consumer_id, boot.clone()));
            }
        }
    }
    out
}

/// Idempotence guard for `reconcile_legacy_topics`: the set of legacy det topics
/// we've already LEFT, so the grace-expiry teardown runs exactly once per topic.
/// Process-global (native runs across OS threads, so a thread_local would let
/// each poll thread re-fire the leave) — matches the `dedup_index` idiom.
fn left_legacy_topics() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static LEFT: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    LEFT.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// F-LEGACY-PAIR-TOPIC (re-fix): actually LEAVE the leaky legacy deterministic
/// pair topic once its SELF-owned grace has lapsed — `my_v2_topics` merely STOPS
/// returning it, but `peer_receiver` only ever ADDS topics, so without an explicit
/// leave the gossip provider stays subscribed to the DID-derivable
/// `SHA256(DID‖DID)` topic forever (the metadata leak survives the timeout). This
/// runs on the poll cadence over ALL Regular-mode contacts, peer-independently:
/// for each contact whose `salted_self_ready_at` grace has elapsed (and only then,
/// so an in-grace / not-yet-migrated peer is never torn down), it leaves the
/// legacy topic + purges its outbox EXACTLY ONCE (guarded by `left_legacy_topics`,
/// and `forget_topic` is itself a no-op if already left). The salted topic stays
/// joined via `my_v2_topics`, so nothing inbound is stranded.
pub async fn reconcile_legacy_topics() {
    let now = now_ms();
    let my_did = ensure_profile().await.map(|m| m.did_key).unwrap_or_default();
    if my_did.is_empty() {
        return;
    }
    for c in list_contacts().await {
        if !matches!(c.mode, IdentityMode::Regular) {
            continue;
        }
        // Drive the teardown off the SAME predicate as the listen-side gate in `my_v2_topics`:
        // the peer has migrated its SENDS (`peer_salted`), we can derive the salted topic, AND the
        // bounded grace has elapsed. peer_salted is REQUIRED (F-11 fix) — tearing legacy down on a
        // self-only timer stranded peers that were still sending on it (mutual silent drop).
        let migrated_off_legacy = c.salted_queue.is_some()
            && c.peer_salted
            && c.salted_self_ready_at != 0
            && now - c.salted_self_ready_at > LEGACY_TOPIC_GRACE_MS;
        if !migrated_off_legacy {
            continue;
        }
        let det = pair_inbound_queue(&my_did, &c.did);
        let topic = format!("{TOPIC_PREFIX_V2}/{det}");
        // Skip if we've already torn this topic down — avoids the redundant async
        // round-trips every poll. (forget_topic is itself a no-op if already left,
        // so this guard is an optimization, not a correctness requirement.)
        let already = {
            let Ok(set) = left_legacy_topics().lock() else {
                continue;
            };
            set.contains(&topic)
        };
        if already {
            continue;
        }
        // forget_topic drops the topic from JOINED_TOPICS + tells the gossip
        // provider to unsubscribe — THIS is what actually closes the leak (omitting
        // it from my_v2_topics alone never unsubscribes). purge_topic clears any
        // stranded outbox entries for the dead topic. Mark the guard only AFTER the
        // teardown so a transient leave failure is retried on the next poll.
        crate::peer_receiver::forget_topic(&topic).await;
        crate::api::outbox::purge_topic(&topic).await;
        if let Ok(mut set) = left_legacy_topics().lock() {
            set.insert(topic);
        }
    }
}
