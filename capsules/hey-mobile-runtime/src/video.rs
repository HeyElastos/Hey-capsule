//! Video-call plane (direct-only) — H.264 frames over QUIC **unidirectional
//! streams** on the `hey/video/1` ALPN, sharing the carrier's one iroh endpoint.
//! PARALLEL to voice: a video call is a voice call with this lane also up; nothing
//! here touches the audio path, and tearing video down leaves audio intact.
//!
//! Frames are OPAQUE here — the Kotlin codec owns H.264 (target 1080p adaptive).
//! This plane just ships `[u32 LE len][frame]…` and enforces the same per-call
//! auth as voice.
//!
//! TRANSPORT — one long-lived uni-stream per direction:
//!   * Streams (not datagrams) so an arbitrarily large 1080p keyframe ships with
//!     NO app-level fragmentation, and frames stay strictly in order for clean
//!     H.264 decode (P-frames need their predecessors).
//!   * On a normal (low-loss) DIRECT link there is no head-of-line stall, so the
//!     end-to-end latency is ~one frame — "no lag in normal connectivity".
//!   * The inbound queue is BOUNDED and drops the OLDEST on overflow, so a slow
//!     decoder produces a brief stutter, never a growing lag; the codec layer
//!     re-keys on a detected gap. The outbound queue is bounded + drops on
//!     overflow so encode can never outrun the network into unbounded memory.
//!
//! SECURITY — `bind()` copies voice.rs's ROSTER gate VERBATIM: inbound is accepted
//! ONLY from the authorized call peer, so a contact merely holding our carrier
//! ticket cannot join the video stream. Direct-only is hard-gated at start (a
//! known-relay peer is refused) and offered only on a direct path by the UI.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId};
use tokio::sync::mpsc;

pub const VIDEO_ALPN: &[u8] = b"hey/video/1";

/// Inbound decoded-frame slack per peer. Overflow drops the OLDEST (a stutter,
/// not a growing lag); the codec re-keys on the gap.
const INBOUND_CAP: usize = 8;
/// Outbound frames waiting for the writer. Overflow drops the frame so encode
/// never outruns the network into unbounded memory (real-time: skip, don't queue).
const OUTBOUND_CAP: usize = 16;
/// Anti-OOM ceiling on a single wire frame. A 1080p keyframe is well under this;
/// anything larger is malformed/hostile → drop the connection.
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

static PEERS: OnceLock<Mutex<HashMap<EndpointId, Connection>>> = OnceLock::new();
static INBOUND: OnceLock<Mutex<HashMap<EndpointId, VecDeque<Vec<u8>>>>> = OnceLock::new();
static OUTBOX: OnceLock<Mutex<HashMap<EndpointId, mpsc::Sender<Vec<u8>>>>> = OnceLock::new();
static ROSTER: OnceLock<Mutex<HashSet<EndpointId>>> = OnceLock::new();
static DIALING: OnceLock<Mutex<HashSet<EndpointId>>> = OnceLock::new();
/// Bumped on every start/stop so stale recv/writer loops + late dials die.
static GEN: AtomicU64 = AtomicU64::new(0);
/// Camera-off: stop emitting frames without tearing the lane down.
static PAUSED: AtomicBool = AtomicBool::new(false);
/// Frames dropped at the send queue (network behind) — the feedback signal the
/// Kotlin adaptive-bitrate loop reads to back off before the link actually lags.
static DROPPED: AtomicU64 = AtomicU64::new(0);
/// Bumped every time a NEW peer subscribes (a writer/outbox is created in `bind`).
/// The Kotlin sync loop watches this and asks the shared encoder for an immediate
/// keyframe so a late joiner's decoder configures from SPS/PPS at once instead of
/// waiting up to a full GOP (~2s) for the next I-frame → black tile.
static NEW_PEER_EPOCH: AtomicU64 = AtomicU64::new(0);
/// Connections currently parked in the roster-grace loop. Bounds how many unauthorized-
/// yet-plausible binds can hold a QUIC connection alive at once, so an inbound flood can't
/// spawn unbounded ~8s grace tasks (accept-amplification). A legit joiner in an active call
/// is one or two; the ceiling is generous yet finite.
static IN_GRACE: AtomicUsize = AtomicUsize::new(0);
/// Max concurrent connections allowed in the roster-grace window.
const MAX_IN_GRACE: usize = 8;

// ── per-call app-layer media E2E (1:1) ───────────────────────────────────────
// Mirrors voice.rs. Set after the PQ-DM call-offer key exchange (with a VIDEO-specific key, so it
// can never share a nonce space with the voice stream); None ⇒ frames go PLAINTEXT (legacy/group).
struct MediaKeys { tx: [u8; 32], rx: [u8; 32] }
static MEDIA: OnceLock<Mutex<Option<MediaKeys>>> = OnceLock::new();
static MEDIA_TX_CTR: AtomicU64 = AtomicU64::new(0);
static MEDIA_RX_HI: AtomicU64 = AtomicU64::new(0);
const MEDIA_REPLAY_WINDOW: u64 = 512;
fn media() -> &'static Mutex<Option<MediaKeys>> {
    MEDIA.get_or_init(|| Mutex::new(None))
}
/// Install the per-call directional VIDEO keys (1:1 E2E). Resets the nonce counters.
pub fn set_media_keys(tx: [u8; 32], rx: [u8; 32]) {
    *crate::lock_safe(media()) = Some(MediaKeys { tx, rx });
    MEDIA_TX_CTR.store(0, Ordering::SeqCst);
    MEDIA_RX_HI.store(0, Ordering::SeqCst);
}
/// Drop the video keys (call end) — subsequent un-keyed video is plaintext again.
pub fn clear_media_keys() {
    *crate::lock_safe(media()) = None;
}

// ── per-call GROUP video E2E (fail-closed; mirrors voice.rs) ──────────────────
// One shared per-call key + a per-sender 4-byte nonce salt. FAIL-CLOSED: a key-holder ALWAYS seals
// its outbound frames and DROPS any inbound frame that doesn't open — a member self-asserting "not
// media-capable" can't downgrade a keyed call; it is simply unheard (excluded), never fed plaintext.
// Only a call with NO group key at all (true legacy) sends + renders plaintext. ACTIVE = UI hint only.
struct GroupMediaKey {
    key: [u8; 32],
    salt: [u8; 4],
}
static GROUP_MEDIA: OnceLock<Mutex<Option<GroupMediaKey>>> = OnceLock::new();
static GROUP_MEDIA_ACTIVE: AtomicBool = AtomicBool::new(false);
static GROUP_MEDIA_TX_CTR: AtomicU64 = AtomicU64::new(0);
/// Per-sender (by nonce salt) high-water counter for group RX replay defense.
static GROUP_RX_HI: OnceLock<Mutex<HashMap<[u8; 4], u64>>> = OnceLock::new();
fn group_media() -> &'static Mutex<Option<GroupMediaKey>> {
    GROUP_MEDIA.get_or_init(|| Mutex::new(None))
}
fn group_rx_hi() -> &'static Mutex<HashMap<[u8; 4], u64>> {
    GROUP_RX_HI.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Install the shared group-video key + our sender salt (idempotent on the same key → keeps the
/// monotonic tx counter so a re-install never reuses a nonce).
pub fn set_group_media_key(key: [u8; 32], salt: [u8; 4]) {
    let mut g = crate::lock_safe(group_media());
    if g.as_ref().map(|k| k.key) == Some(key) {
        return;
    }
    *g = Some(GroupMediaKey { key, salt });
    GROUP_MEDIA_TX_CTR.store(0, Ordering::SeqCst);
}
/// UI hint only ("all participants can decrypt"). Does NOT gate sealing — sealing is fail-closed on
/// key possession (see the send path), so a self-asserted incapable member can't strip encryption.
pub fn set_group_media_active(active: bool) {
    GROUP_MEDIA_ACTIVE.store(active, Ordering::SeqCst);
}
pub fn clear_group_media_keys() {
    *crate::lock_safe(group_media()) = None;
    GROUP_MEDIA_ACTIVE.store(false, Ordering::SeqCst);
    crate::lock_safe(group_rx_hi()).clear();
}

fn peers() -> &'static Mutex<HashMap<EndpointId, Connection>> {
    PEERS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn inbound() -> &'static Mutex<HashMap<EndpointId, VecDeque<Vec<u8>>>> {
    INBOUND.get_or_init(|| Mutex::new(HashMap::new()))
}
fn outbox() -> &'static Mutex<HashMap<EndpointId, mpsc::Sender<Vec<u8>>>> {
    OUTBOX.get_or_init(|| Mutex::new(HashMap::new()))
}
fn roster() -> &'static Mutex<HashSet<EndpointId>> {
    ROSTER.get_or_init(|| Mutex::new(HashSet::new()))
}
fn dialing() -> &'static Mutex<HashSet<EndpointId>> {
    DIALING.get_or_init(|| Mutex::new(HashSet::new()))
}

fn reset_session() {
    GEN.fetch_add(1, Ordering::SeqCst);
    let conns: Vec<Connection> = crate::lock_safe(peers()).drain().map(|(_, c)| c).collect();
    for c in conns {
        c.close(0u32.into(), b"bye");
    }
    crate::lock_safe(inbound()).clear();
    crate::lock_safe(outbox()).clear(); // drops senders → writer tasks exit
    crate::lock_safe(roster()).clear();
    crate::lock_safe(dialing()).clear();
    PAUSED.store(false, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    clear_group_media_keys(); // fresh session: app re-installs the group key once the secret arrives
    // NOTE: IN_GRACE is intentionally NOT reset here. Each grace task pairs its own
    // increment with exactly one decrement on every exit path (including the GEN-bump
    // early return), so the counter self-balances. A forced store(0) here would let a
    // still-running old-generation task underflow the counter on its decrement.
}

/// Cumulative frames dropped at the send queue this process — the adaptive loop
/// watches the delta to back the bitrate off (then recover) before the link lags.
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Monotonic counter bumped each time a NEW peer subscribes (an outbox is created in
/// `bind`). The Kotlin sync loop watches the delta and requests an immediate keyframe
/// so a late joiner isn't a black tile until the next GOP.
pub fn new_peer_epoch() -> u64 {
    NEW_PEER_EPOCH.load(Ordering::Relaxed)
}

/// Begin a 1:1 video session with `peer` (a strict 1-peer mesh, mirroring voice).
/// Both sides call this; the smaller EndpointId dials. Must run on the carrier runtime.
pub async fn start(endpoint: Endpoint, peer: EndpointId) {
    reset_session();
    crate::lock_safe(roster()).insert(peer);
    let g = GEN.load(Ordering::SeqCst);
    log::info!("video: session start (we dial iff our id < peer id)"); // peer/endpoint ids redacted
    maybe_dial(endpoint.clone(), peer, g);
    // Self-healing re-dial: keep trying for the WHOLE session whenever there is no
    // live link. CRITICAL: do NOT terminate just because peers() momentarily holds the
    // peer. bind() inserts into peers() BEFORE its reader runs, so a dial the far side
    // rejects — its roster not ready yet, which is exactly what happens when WE are the
    // callee/dialer and the CALLER hasn't started its video session yet — shows up in
    // peers() for a blink, then cleans up. The old loop did `return` on that blink and
    // permanently gave up, so once the caller finally WAS ready nobody dialed again →
    // "video link not forming" / one-directional. Now we only SKIP that tick and retry,
    // and we live as long as the session (GEN) does.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            if GEN.load(Ordering::SeqCst) != g {
                return;
            }
            if !crate::lock_safe(peers()).contains_key(&peer) {
                maybe_dial(endpoint.clone(), peer, g);
            }
        }
    });
}

/// Begin a GROUP video session: an empty mesh. Participants are authorized + dialed
/// as the signed group-call roster syncs in via [`sync_peers`] (mirrors
/// `voice::group_start`). Inbound from anyone NOT in the synced roster is rejected.
pub fn group_start() {
    reset_session();
}

/// Reconcile the video mesh toward `wanted` (the live participant roster, minus self):
/// authorize + (re)dial any peer not already connected, AND evict any peer that has left
/// the wanted set (a kicked/barred member — `group_call_roster` already excludes
/// barred/removed members). Called repeatedly (~every 1.5s) by the native CallManager as
/// it polls the group-call roster — that repetition is what re-dials a peer whose first
/// dial lost the roster-sync race AND what tears down a removed one. Mirrors
/// `voice::sync_peers`. Eviction closes that peer's connection and drops its outbox so
/// send_frame stops shipping it H.264 (F-GCALL-BARRED). An EMPTY `wanted` is treated as a
/// transient roster-read gap and skips eviction so a momentary read failure can't freeze
/// every tile (stop()/leave tears the session down instead).
pub fn sync_peers(endpoint: Endpoint, wanted: Vec<EndpointId>) {
    let g = GEN.load(Ordering::SeqCst);
    let wanted_set: HashSet<EndpointId> = wanted.iter().copied().collect();
    // ── EVICT peers no longer wanted (kicked/barred/left) ──
    // sync_peers receives the FULL authoritative participant set every ~1.5s poll, so any peer
    // currently authorized/connected but absent from `wanted` was removed and MUST be torn down —
    // otherwise sync_peers stays INSERT-ONLY and a barred member keeps receiving live video.
    // Skip when `wanted` is empty (transient gap) to preserve legit participants.
    if !wanted_set.is_empty() {
        let mut tracked: HashSet<EndpointId> = crate::lock_safe(roster()).iter().copied().collect();
        tracked.extend(crate::lock_safe(peers()).keys().copied());
        for id in tracked {
            if wanted_set.contains(&id) {
                continue; // still a legit participant — leave it alone
            }
            // Revoke authorization first so any in-flight bind() for this peer is rejected.
            crate::lock_safe(roster()).remove(&id);
            crate::lock_safe(dialing()).remove(&id);
            // Close the connection (stops inbound frames; the reader exits on the closed conn) and
            // drop the outbox (send_frame iterates outbox values → removing it stops shipping this
            // peer H.264) + the inbound queue.
            if let Some(conn) = crate::lock_safe(peers()).remove(&id) {
                conn.close(0u32.into(), b"removed");
            }
            crate::lock_safe(outbox()).remove(&id); // drops the sender → its writer task exits
            crate::lock_safe(inbound()).remove(&id);
        }
    }
    // ── ADD/keep wanted peers ──
    for p in wanted {
        crate::lock_safe(roster()).insert(p);
        if crate::lock_safe(peers()).contains_key(&p) {
            continue;
        }
        maybe_dial(endpoint.clone(), p, g);
    }
}

/// Number of LIVE video links — the UI's "connecting video…" probe (0 while dialing).
pub fn connected_peers() -> usize {
    crate::lock_safe(peers()).len()
}

/// EndpointIds with a LIVE video link, so the grid UI can build one tile per remote
/// and pull that peer's frames via [`recv_frame_from`].
pub fn peer_ids() -> Vec<String> {
    // SORTED so the order is STABLE across polls. HashMap key order is
    // non-deterministic; the Kotlin grid maps peers to POSITIONAL tiles, so an
    // unsorted Vec churns the grid (reordering tiles + rebuilding decoders) every
    // poll. A stable order keeps each peer pinned to its tile.
    let mut v: Vec<String> = crate::lock_safe(peers()).keys().map(|id| id.to_string()).collect();
    v.sort();
    v
}

/// Dial `peer`'s video ALPN iff our id sorts first (polite-peer tie-break, so each
/// pair has exactly one connection); otherwise wait for their inbound.
fn maybe_dial(endpoint: Endpoint, peer: EndpointId, g: u64) {
    if endpoint.id().to_string() >= peer.to_string() {
        return;
    }
    if !crate::lock_safe(dialing()).insert(peer) {
        return; // one in-flight dial per peer
    }
    tokio::spawn(async move {
        let r = endpoint.connect(peer, VIDEO_ALPN).await;
        crate::lock_safe(dialing()).remove(&peer);
        match r {
            Ok(conn) => bind(conn, g).await,
            Err(e) => log::warn!("video: dial failed: {e}") /* peer id redacted */,
        }
    });
}

/// Honor an established connection for the current session: register it, spawn the
/// writer (drains the outbound queue → our uni-stream) and run the reader (the
/// peer's uni-stream → inbound queue). Rejects stray/late/unauthorized connections.
async fn bind(conn: Connection, g: u64) {
    if GEN.load(Ordering::SeqCst) != g {
        return;
    }
    let id = conn.remote_id();
    // EAVESDROP GATE (copied verbatim from voice.rs:204): accept ONLY the
    // authorized call peer, so a ticket-holder cannot join the video stream.
    // EAVESDROP GATE (from voice.rs): accept ONLY the authorized call peer. BUT the signed
    // gcall roster can LAG behind the QUIC connection — especially when the polite-peer DIALER
    // is a JOINER and WE are the acceptor (the caller whose call-roster hasn't yet received the
    // joiner's "join" announce). The old code rejected + DROPPED the conn here, forcing a re-dial
    // race against roster-sync → persistent one-way black for the joiner (Pixel-8-caller +
    // Pixel-10-joiner). Fix: HOLD the connection and re-check the roster over a short grace
    // window so we accept the instant the join propagates (we still own `conn`, so iroh keeps it
    // open). A true stranger never enters the roster, so it still times out and is rejected.
    if !crate::lock_safe(roster()).contains(&id) {
        // ACCEPT-AMPLIFICATION GUARD: the grace loop below holds an inbound QUIC
        // connection alive for ~8s. Only pay that cost when this is PLAUSIBLY a legit
        // joiner whose signed roster just hasn't propagated yet — i.e. a call is
        // actually live RIGHT NOW (we already hold >=1 known participant in roster).
        // When the roster is empty there is no active call, so an inbound from an
        // unknown peer is a stranger: reject INSTANTLY like voice.rs (no grace, no
        // held connection). This still lets a joiner connect within the window of an
        // active group call, where roster already lists the existing participants.
        if crate::lock_safe(roster()).is_empty() {
            log::warn!("video: rejected — no active call (instant reject)");
            return;
        }
        // Bound the number of connections allowed to sit in grace at once so an inbound
        // flood can't spawn unbounded ~8s grace tasks. Reserve a slot up front; reject
        // immediately if the ceiling is reached.
        if IN_GRACE.fetch_add(1, Ordering::SeqCst) >= MAX_IN_GRACE {
            IN_GRACE.fetch_sub(1, Ordering::SeqCst);
            log::warn!("video: rejected — grace capacity reached (instant reject)");
            return;
        }
        let mut authorized = false;
        for _ in 0..32 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await; // up to ~8s grace
            if GEN.load(Ordering::SeqCst) != g {
                IN_GRACE.fetch_sub(1, Ordering::SeqCst);
                return;
            }
            if crate::lock_safe(roster()).contains(&id) {
                authorized = true;
                break;
            }
        }
        IN_GRACE.fetch_sub(1, Ordering::SeqCst);
        if !authorized {
            log::warn!("video: rejected — not in the call roster after grace");
            return;
        }
        log::info!("video: peer authorized after roster grace");
    }
    let sid = conn.stable_id();
    log::info!("video: link UP");
    crate::lock_safe(peers()).insert(id, conn.clone());
    crate::lock_safe(inbound()).entry(id).or_default();

    // WRITER: one uni-stream out, length-prefixed frames as they are queued.
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_CAP);
    crate::lock_safe(outbox()).insert(id, tx);
    // A new subscriber just got an outbox → signal the encoder to emit a keyframe NOW
    // so this late joiner's decoder configures immediately instead of dropping every
    // P-frame until the next ~2s GOP boundary (the black-tile fix).
    NEW_PEER_EPOCH.fetch_add(1, Ordering::Relaxed);
    {
        let conn_w = conn.clone();
        tokio::spawn(async move {
            let mut send = match conn_w.open_uni().await {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("video: open_uni failed: {e}");
                    return;
                }
            };
            // CRITICAL: a QUIC uni-stream is INVISIBLE to the peer's accept_uni() until
            // the opener WRITES data — open_uni() alone sends nothing on the wire. So
            // flush a 0-length header IMMEDIATELY: this makes our outbound direction
            // observable the instant the link is up, even before (or without) any
            // encoded frame — fixing the one-directional blackout where a side that
            // isn't producing frames yet (camera warming, paused, receive-only, or a
            // failed encoder) leaves the peer's reader hanging forever. The reader
            // treats len==0 as a keepalive, not data.
            if send.write_all(&0u32.to_le_bytes()).await.is_err() {
                return;
            }
            while GEN.load(Ordering::SeqCst) == g {
                match tokio::time::timeout(std::time::Duration::from_millis(1000), rx.recv()).await
                {
                    Ok(Some(frame)) => {
                        // E2E: seal with the per-call VIDEO key when set; else, if a per-call GROUP key
                        // is installed, ALWAYS seal with it (FAIL CLOSED — a key-holder never emits
                        // plaintext, so a self-asserted "not media-capable" member can't downgrade the
                        // room; it is just unheard, never fed plaintext). Only a call with NO group key
                        // (true legacy) ships plaintext H.264. The wire len is the SEALED length; a
                        // 0-len keepalive (below) is never sealed, so len==0 stays a control marker.
                        let wire: Vec<u8> = {
                            let guard = crate::lock_safe(media());
                            if let Some(k) = guard.as_ref() {
                                let ctr = MEDIA_TX_CTR.fetch_add(1, Ordering::SeqCst);
                                hey_core::crypto::media_seal(&k.tx, ctr, &frame)
                            } else {
                                drop(guard);
                                let g2 = crate::lock_safe(group_media());
                                if let Some(gk) = g2.as_ref() {
                                    let ctr = GROUP_MEDIA_TX_CTR.fetch_add(1, Ordering::SeqCst);
                                    hey_core::crypto::media_group_seal(&gk.key, gk.salt, ctr, &frame)
                                } else {
                                    frame
                                }
                            }
                        };
                        let len = wire.len() as u32;
                        if send.write_all(&len.to_le_bytes()).await.is_err() {
                            break;
                        }
                        if send.write_all(&wire).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break, // session ended (sender dropped)
                    Err(_) => {
                        // Idle 1s (no frame yet / camera off / paused): send a 0-length
                        // keepalive so the stream stays observable and the path stays
                        // warm. Cheap; the reader skips it.
                        if send.write_all(&0u32.to_le_bytes()).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = send.finish();
        });
    }

    // READER: the peer's uni-stream in, length-prefixed frames → bounded queue.
    let reader = async {
        let mut recv = conn
            .accept_uni()
            .await
            .map_err(|e| format!("accept_uni: {e}"))?;
        loop {
            if GEN.load(Ordering::SeqCst) != g {
                break;
            }
            let mut hdr = [0u8; 4];
            if recv.read_exact(&mut hdr).await.is_err() {
                break;
            }
            let len = u32::from_le_bytes(hdr) as usize;
            // +64 allows the AEAD counter+tag overhead on a sealed frame (see crypto::media_seal).
            if len > MAX_FRAME_BYTES + 64 {
                break; // malformed / hostile → drop the connection
            }
            if len == 0 {
                continue; // keepalive — the stream is alive even before frames flow
            }
            let mut frame = vec![0u8; len];
            if recv.read_exact(&mut frame).await.is_err() {
                break;
            }
            // E2E: open with the per-call VIDEO key when set; drop undecryptable (fail closed) +
            // bounded replay. No key ⇒ treat as plaintext H.264 (legacy peer / group).
            let frame: Vec<u8> = {
                let guard = crate::lock_safe(media());
                if let Some(k) = guard.as_ref() {
                    // 1:1 keyed ⇒ FAIL-CLOSED.
                    match hey_core::crypto::media_open(&k.rx, &frame) {
                        Some((ctr, pt)) => {
                            let hi = MEDIA_RX_HI.load(Ordering::SeqCst);
                            if ctr + MEDIA_REPLAY_WINDOW < hi {
                                continue; // too old → drop (replay guard)
                            }
                            if ctr > hi {
                                MEDIA_RX_HI.store(ctr, Ordering::SeqCst);
                            }
                            pt
                        }
                        None => continue, // bad tag / wrong key → drop, fail closed
                    }
                } else {
                    drop(guard);
                    // GROUP: if we hold the group key, FAIL CLOSED — drop any frame that doesn't open
                    // (never render plaintext into a keyed call). Only a call with NO group key at all
                    // (true legacy / un-keyed) renders plaintext H.264.
                    let gkey = crate::lock_safe(group_media()).as_ref().map(|g| g.key);
                    match gkey {
                        Some(key) => match hey_core::crypto::media_group_open(&key, &frame) {
                            Some((salt, ctr, pt)) => {
                                // Per-sender replay window (freshness; AEAD integrity already holds).
                                let mut hi = crate::lock_safe(group_rx_hi());
                                let h = hi.entry(salt).or_insert(0);
                                if ctr + MEDIA_REPLAY_WINDOW < *h {
                                    continue; // too old / replayed → drop
                                }
                                if ctr > *h {
                                    *h = ctr;
                                }
                                pt
                            }
                            None => continue, // fail closed: hold the key ⇒ never render undecryptable bytes
                        },
                        None => frame, // no group key at all → true legacy, plaintext
                    }
                }
            };
            let mut q = crate::lock_safe(inbound());
            let dq = q.entry(id).or_default();
            dq.push_back(frame);
            // Bounded queue, but NEVER evict a keyframe to make room. The wire's first byte
            // is the flags byte (bit0 = keyframe); a head keyframe carries SPS/PPS and is the
            // ONLY frame that can (re)configure a peer's decoder. The old unconditional
            // pop_front could drop that keyframe under a drain stall → the decoder is then fed
            // P-frames into an unconfigured codec and the group tile stays black until the next
            // GOP (or forever if it keeps happening). Instead drop the OLDEST P-frame; only if
            // the whole queue is keyframes (degenerate) fall back to dropping the oldest.
            while dq.len() > INBOUND_CAP {
                let drop_idx = dq
                    .iter()
                    .position(|f| f.first().map_or(true, |b| b & 1 == 0))
                    .unwrap_or(0);
                dq.remove(drop_idx);
            }
        }
        Ok::<(), String>(())
    };
    let _ = reader.await;

    // Tear down ONLY if WE are still the live connection for this peer. A newer
    // re-dial may have SUPERSEDED us (common when we re-dialed repeatedly because the
    // caller wasn't ready yet, then one finally connected). Without this stable_id
    // guard a stale connection's cleanup clobbers the live one's state → peers drops
    // to 0 mid-call and one direction of video freezes. A superseded conn is a no-op.
    if GEN.load(Ordering::SeqCst) == g {
        let mut p = crate::lock_safe(peers());
        if p.get(&id).map(|c| c.stable_id()) == Some(sid) {
            p.remove(&id);
            drop(p);
            crate::lock_safe(inbound()).remove(&id);
            crate::lock_safe(outbox()).remove(&id);
        }
    }
}

/// Queue one encoded frame for delivery to every peer (1:1 = one). Pure sync —
/// called from the Kotlin encoder thread. Dropped if paused or the writer is
/// behind (real-time: skip a frame rather than build latency; the codec re-keys).
pub fn send_frame(frame: &[u8]) {
    if PAUSED.load(Ordering::Relaxed) || frame.is_empty() {
        return;
    }
    let senders: Vec<mpsc::Sender<Vec<u8>>> =
        crate::lock_safe(outbox()).values().cloned().collect();
    for s in senders {
        if s.try_send(frame.to_vec()).is_err() {
            // Network behind → frame dropped. The adaptive loop reads `dropped()`
            // to lower the bitrate; the codec re-keys on the resulting gap.
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Pop the next received frame (in order) for the decoder, or empty if none ready.
/// 1:1 path — peer-agnostic (drains whichever peer has a frame).
pub fn recv_frame() -> Vec<u8> {
    let mut q = crate::lock_safe(inbound());
    for dq in q.values_mut() {
        if let Some(f) = dq.pop_front() {
            return f;
        }
    }
    Vec::new()
}

/// Pop the next received frame from a SPECIFIC peer (the group grid: each remote
/// tile decodes its own peer's stream in order). Empty if none ready. `peer` is the
/// EndpointId string from [`peer_ids`].
pub fn recv_frame_from(peer: &str) -> Vec<u8> {
    let mut q = crate::lock_safe(inbound());
    for (id, dq) in q.iter_mut() {
        if id.to_string() == peer {
            return dq.pop_front().unwrap_or_default();
        }
    }
    Vec::new()
}

pub fn set_paused(p: bool) {
    PAUSED.store(p, Ordering::Relaxed);
}

pub fn stop() {
    reset_session();
}

// ── carrier Router protocol handler (accept inbound video connections) ──
#[derive(Debug, Clone)]
pub struct VideoProtocol;

impl iroh::protocol::ProtocolHandler for VideoProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), iroh::protocol::AcceptError> {
        let g = GEN.load(Ordering::SeqCst);
        bind(conn, g).await;
        Ok(())
    }
}
