//! Voice-call audio plane (Stage 2) — mesh-capable (1:1 **and** group calls).
//!
//! μ-law (G.711) 8 kHz mono over QUIC **datagrams** (unreliable → low latency, no head-of-line
//! blocking) on the `hey/voice/1` ALPN, which shares the carrier's iroh endpoint — so calls are the
//! same NAT-traversed, encrypted, serverless P2P as everything else. ~64 kbps, cellular-friendly.
//!
//! A call is a **full mesh**: one QUIC connection to every other participant. 1:1 is just a 1-peer
//! mesh. Captured PCM is broadcast to every peer; inbound streams are summed + clamped into one mix
//! for playback. To avoid a duplicate connection per pair, the peer with the smaller EndpointId
//! dials and the other accepts (the "polite peer" tie-break).
//!
//! Kotlin owns audio I/O: AudioRecord (VOICE_COMMUNICATION source → hardware echo-cancel/AGC/NS)
//! pushes captured PCM via `send_pcm`; AudioTrack pulls the decoded mix via `recv_pcm`. send/recv
//! are plain sync calls (no runtime needed); only dialing touches async.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use bytes::Bytes;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId};

pub const VOICE_ALPN: &[u8] = b"hey/voice/1";

/// ~1s of 8 kHz audio — caps each peer's jitter buffer so a stall can't grow it without bound.
const JITTER_CAP: usize = 8000;

/// Live voice connections, keyed by the remote participant's id.
static PEERS: OnceLock<Mutex<HashMap<EndpointId, Connection>>> = OnceLock::new();
/// Per-peer decoded jitter buffers — mixed together on `recv_pcm`.
static PEER_BUFS: OnceLock<Mutex<HashMap<EndpointId, VecDeque<i16>>>> = OnceLock::new();
/// Peers AUTHORIZED in the current call — 1:1 = the single peer, group = the
/// synced participant roster. Inbound connections are accepted ONLY from peers
/// in here (in group calls the roster is built from signed group-thread control
/// messages via `sync_peers`, so a stranger holding our ticket can't join).
static ROSTER: OnceLock<Mutex<HashSet<EndpointId>>> = OnceLock::new();
/// Peers we currently have an in-flight dial to — one dial per peer across the
/// ~1.5s reconcile ticks, so retries don't pile up concurrent connects.
static DIALING: OnceLock<Mutex<HashSet<EndpointId>>> = OnceLock::new();
/// Bumped on every start/stop so stale recv loops exit and late dials/accepts for an ended call die.
static GEN: AtomicU64 = AtomicU64::new(0);
static MUTED: AtomicBool = AtomicBool::new(false);

// ── per-call app-layer media E2E (1:1) ───────────────────────────────────────
// Set after the PQ-DM call-offer key exchange; cleared on call end. None ⇒ frames go PLAINTEXT
// (legacy peer / group / verse, not keyed yet). The fresh per-call key + a strictly-monotonic tx
// counter guarantee the ChaCha20-Poly1305 nonce never repeats.
struct MediaKeys { tx: [u8; 32], rx: [u8; 32] }
static MEDIA: OnceLock<Mutex<Option<MediaKeys>>> = OnceLock::new();
static MEDIA_TX_CTR: AtomicU64 = AtomicU64::new(0);
static MEDIA_RX_HI: AtomicU64 = AtomicU64::new(0);
/// Accept reorder/jitter within this many frames behind the highest seen; older ⇒ drop (bounds replay).
const MEDIA_REPLAY_WINDOW: u64 = 512;
fn media() -> &'static Mutex<Option<MediaKeys>> {
    MEDIA.get_or_init(|| Mutex::new(None))
}
/// Install the per-call directional media keys (1:1 E2E). `tx` seals our outbound frames; `rx`
/// opens the peer's. Resets the nonce counters for the new call.
pub fn set_media_keys(tx: [u8; 32], rx: [u8; 32]) {
    *crate::lock_safe(media()) = Some(MediaKeys { tx, rx });
    MEDIA_TX_CTR.store(0, Ordering::SeqCst);
    MEDIA_RX_HI.store(0, Ordering::SeqCst);
}
/// Drop the media keys (call end) — subsequent un-keyed media is plaintext again.
pub fn clear_media_keys() {
    *crate::lock_safe(media()) = None;
}

// ── per-call GROUP media E2E (fail-closed) ────────────────────────────────────
// ONE shared per-call key (every member derives the same from the sealed call secret) + a per-sender
// 4-byte nonce salt so distinct senders never collide a (key, nonce). FAIL-CLOSED: a member holding
// the group key ALWAYS seals its outbound frames and DROPS any inbound frame that does not open — so a
// member self-asserting "not media-capable" can no longer downgrade a keyed call to plaintext; it is
// simply unheard by key-holders (excluded), never fed/served plaintext. Only a call with NO group key
// at all (true legacy / verse) sends + accepts plaintext. GROUP_MEDIA_ACTIVE is now a UI hint only.
struct GroupMediaKey {
    key: [u8; 32],
    salt: [u8; 4],
}
static GROUP_MEDIA: OnceLock<Mutex<Option<GroupMediaKey>>> = OnceLock::new();
static GROUP_MEDIA_ACTIVE: AtomicBool = AtomicBool::new(false);
static GROUP_MEDIA_TX_CTR: AtomicU64 = AtomicU64::new(0);
/// Per-SENDER (keyed by nonce salt) high-water counter for group RX replay defense — the group
/// path has many senders, so unlike the single 1:1 MEDIA_RX_HI it needs a per-salt map.
static GROUP_RX_HI: OnceLock<Mutex<HashMap<[u8; 4], u64>>> = OnceLock::new();
fn group_media() -> &'static Mutex<Option<GroupMediaKey>> {
    GROUP_MEDIA.get_or_init(|| Mutex::new(None))
}
fn group_rx_hi() -> &'static Mutex<HashMap<[u8; 4], u64>> {
    GROUP_RX_HI.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Install the shared group-media key + OUR sender salt. Idempotent on the SAME key (keeps the
/// monotonic tx counter so a periodic re-install can never reuse a nonce); a new key resets it.
pub fn set_group_media_key(key: [u8; 32], salt: [u8; 4]) {
    let mut g = crate::lock_safe(group_media());
    if g.as_ref().map(|k| k.key) == Some(key) {
        return; // same key — preserve the counter (no nonce reuse)
    }
    *g = Some(GroupMediaKey { key, salt });
    GROUP_MEDIA_TX_CTR.store(0, Ordering::SeqCst);
}
/// UI hint only: "every participant can decrypt" (all media-capable). This NO LONGER gates sealing —
/// sealing is fail-closed on key possession (see send_pcm), so a self-asserted incapable member can't
/// strip encryption. Kept so the app can surface a "not fully encrypted" badge.
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
fn peer_bufs() -> &'static Mutex<HashMap<EndpointId, VecDeque<i16>>> {
    PEER_BUFS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn roster() -> &'static Mutex<HashSet<EndpointId>> {
    ROSTER.get_or_init(|| Mutex::new(HashSet::new()))
}
fn dialing() -> &'static Mutex<HashSet<EndpointId>> {
    DIALING.get_or_init(|| Mutex::new(HashSet::new()))
}

// ── μ-law (G.711) — standard, pure Rust ───────────────────────────────────────
fn linear_to_ulaw(sample: i16) -> u8 {
    const BIAS: i32 = 0x84;
    const CLIP: i32 = 32635;
    let mut s = sample as i32;
    let sign = if s < 0 {
        s = -s;
        0x80
    } else {
        0
    };
    if s > CLIP {
        s = CLIP;
    }
    s += BIAS;
    let mut exponent = 7;
    let mut mask = 0x4000;
    while exponent > 0 && (s & mask) == 0 {
        exponent -= 1;
        mask >>= 1;
    }
    let mantissa = (s >> (exponent + 3)) & 0x0F;
    !((sign | (exponent << 4) | mantissa) as u8)
}
fn ulaw_to_linear(ulaw: u8) -> i16 {
    let u = !ulaw as i32;
    let sign = (u & 0x80) != 0;
    let exponent = (u >> 4) & 0x07;
    let mantissa = u & 0x0F;
    let mut sample = ((mantissa << 3) + 0x84) << exponent;
    sample -= 0x84;
    if sign {
        (-sample) as i16
    } else {
        sample as i16
    }
}

// ── session lifecycle ─────────────────────────────────────────────────────────

/// Tear down any prior session + start a fresh generation.
fn reset_session() {
    GEN.fetch_add(1, Ordering::SeqCst);
    let conns: Vec<Connection> = crate::lock_safe(peers()).drain().map(|(_, c)| c).collect();
    for c in conns {
        c.close(0u32.into(), b"bye");
    }
    crate::lock_safe(peer_bufs()).clear();
    crate::lock_safe(roster()).clear();
    crate::lock_safe(dialing()).clear();
    MUTED.store(false, Ordering::Relaxed);
    // Fresh session ⇒ no group key yet (the app installs it once the call secret arrives); a 1:1
    // session has none. Clears any stale key from a prior call so counters start clean.
    clear_group_media_keys();
}

/// Begin a **1:1** voice session with `peer` (a strict 1-peer mesh). Both sides call this; the
/// smaller EndpointId dials, the other waits for the inbound. `is_caller` is no longer used for
/// transport (kept for ABI compatibility). Must run on the carrier runtime.
pub async fn start(endpoint: Endpoint, peer: EndpointId, _is_caller: bool) {
    reset_session();
    crate::lock_safe(roster()).insert(peer);
    let g = GEN.load(Ordering::SeqCst);
    maybe_dial(endpoint.clone(), peer, g);
    // 1:1 used to dial exactly ONCE — a single failed handshake (stale paths
    // right after a relay change, a slow hole-punch) meant silence for the
    // whole call. Reconcile like group calls: re-dial every 1.5s until the
    // link is up or the session ends (GEN bump). maybe_dial's polite-peer
    // tie-break still guarantees only one side dials.
    // Self-healing re-dial for the WHOLE session: do NOT give up just because peers()
    // momentarily holds the peer during a rejected dial's bind (the old `return` there
    // permanently stopped dialing → silent / one-directional call). Mirror video.rs:
    // only SKIP a tick when connected; only the GEN bump ends it.
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

/// Number of LIVE voice links in this session — the call screen's "audio
/// connected" probe (0 while the dial is still fighting its way through).
pub fn connected_peers() -> usize {
    crate::lock_safe(peers()).len()
}

/// Begin a **group** voice session: an empty mesh. Participants are authorized + dialed as their
/// signed roster sync arrives via [`sync_peers`] — inbound from anyone NOT in that roster is rejected.
pub fn group_start() {
    reset_session();
}

/// Reconcile the mesh toward `wanted` (the live participant roster, minus self): dial/await any peer
/// not already connected, AND evict any peer that has left the wanted set (a kicked/barred member —
/// `group_call_roster` already excludes barred/removed members). Eviction closes that peer's mesh
/// connection and drops it from the roster + jitter buffers, so it stops receiving live μ-law audio
/// (F-GCALL-BARRED). An EMPTY `wanted` is treated as a transient roster-read gap and skips eviction,
/// so a momentary read failure can't kill an otherwise-live call (stop()/leave tears it down instead).
pub fn sync_peers(endpoint: Endpoint, wanted: Vec<EndpointId>) {
    let g = GEN.load(Ordering::SeqCst);
    let wanted_set: HashSet<EndpointId> = wanted.iter().copied().collect();
    // ── EVICT peers no longer wanted (kicked/barred/left) ──
    // sync_peers receives the FULL authoritative participant set every ~1.5s poll, so any peer
    // currently authorized/connected but absent from `wanted` was removed and MUST be torn down —
    // otherwise sync_peers stays INSERT-ONLY and a barred member keeps receiving live audio.
    // Skip when `wanted` is empty (transient gap) to preserve legit participants.
    if !wanted_set.is_empty() {
        // Snapshot every id we currently track (roster ∪ live conns) so we close the conn AND
        // revoke roster authorization for anyone dropped.
        let mut tracked: HashSet<EndpointId> = crate::lock_safe(roster()).iter().copied().collect();
        tracked.extend(crate::lock_safe(peers()).keys().copied());
        for id in tracked {
            if wanted_set.contains(&id) {
                continue; // still a legit participant — leave it alone
            }
            // Revoke authorization first so any in-flight bind() for this peer is rejected, and a
            // re-add can't race a half-torn-down conn.
            crate::lock_safe(roster()).remove(&id);
            crate::lock_safe(dialing()).remove(&id);
            // Close the mesh connection (stops inbound audio) and drop send/recv state (stops
            // send_pcm to it + frees its jitter buffer). The bind() reader exits on the closed conn.
            if let Some(conn) = crate::lock_safe(peers()).remove(&id) {
                conn.close(0u32.into(), b"removed");
            }
            crate::lock_safe(peer_bufs()).remove(&id);
        }
    }
    // ── ADD/keep wanted peers ──
    for p in wanted {
        // Always (re)assert authorized membership: bind() accepts inbound ONLY from
        // peers in the roster, so every synced participant must be present here.
        crate::lock_safe(roster()).insert(p);
        // Already connected → nothing to do. Otherwise (re)dial: a dial that lost
        // the roster-sync race on the far side would be rejected, and since the
        // dialer only dials once, the pair would never connect without this retry.
        // maybe_dial is a no-op for the non-dialing (larger-id) side and is guarded
        // against duplicate in-flight dials.
        if crate::lock_safe(peers()).contains_key(&p) {
            continue;
        }
        maybe_dial(endpoint.clone(), p, g);
    }
}

/// Dial `peer`'s voice ALPN iff our id sorts before theirs (so exactly one side of each pair dials);
/// otherwise wait for them to dial us. Spawns onto the current (carrier) runtime.
fn maybe_dial(endpoint: Endpoint, peer: EndpointId, g: u64) {
    if endpoint.id().to_string() >= peer.to_string() {
        return; // the other (smaller-id) side dials — polite-peer tie-break
    }
    // One in-flight dial per peer: reconcile runs every ~1.5s and would otherwise
    // spawn a fresh connect each tick for a peer that's still handshaking.
    if !crate::lock_safe(dialing()).insert(peer) {
        return;
    }
    tokio::spawn(async move {
        let r = endpoint.connect(peer, VOICE_ALPN).await;
        crate::lock_safe(dialing()).remove(&peer);
        match r {
            Ok(conn) => bind(conn, g).await,
            Err(e) => log::warn!("voice: dial failed: {e}"), // peer id redacted from logs
        }
    });
}

/// Honor an established connection (dialed or accepted) for the current session: register it + run
/// the receive loop (datagram → μ-law decode → that peer's jitter buffer) until it closes or the
/// session changes. Rejects stray/late connections (wrong generation, or — for 1:1 — not in roster).
async fn bind(conn: Connection, g: u64) {
    if GEN.load(Ordering::SeqCst) != g {
        return;
    }
    let id = conn.remote_id();
    let sid = conn.stable_id();
    // Accept inbound ONLY from a peer authorized in the current call's roster —
    // for 1:1 that's our single peer; for a group it's the synced participant set.
    // Without this, a group call (formerly "open") accepted ANY inbound connection,
    // so any contact holding our carrier ticket could join to eavesdrop or inject audio.
    if !crate::lock_safe(roster()).contains(&id) {
        return;
    }
    crate::lock_safe(roster()).insert(id);
    crate::lock_safe(peers()).insert(id, conn.clone());
    crate::lock_safe(peer_bufs()).entry(id).or_default();
    loop {
        if GEN.load(Ordering::SeqCst) != g {
            break;
        }
        match conn.read_datagram().await {
            Ok(data) => {
                // E2E: open with the per-call key when set; drop undecryptable (fail closed) +
                // bounded replay. No key ⇒ treat as plaintext μ-law (legacy peer / group / verse).
                let ulaw: Vec<u8> = {
                    let guard = crate::lock_safe(media());
                    if let Some(k) = guard.as_ref() {
                        // 1:1 keyed ⇒ FAIL-CLOSED (drop undecryptable).
                        match hey_core::crypto::media_open(&k.rx, data.as_ref()) {
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
                        // GROUP / legacy: try the shared group key; on failure treat the bytes as
                        // PLAINTEXT μ-law (a not-yet-updated member, or a member sending plaintext
                        // because the call isn't all-capable). Graceful — never drops a legit frame.
                        let gkey = crate::lock_safe(group_media()).as_ref().map(|g| g.key);
                        match gkey {
                            Some(key) => match hey_core::crypto::media_group_open(&key, data.as_ref()) {
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
                                // FAIL CLOSED: we hold the group key, so a frame that does NOT open is
                                // dropped — never played as plaintext. Blocks a downgrade/injection by a
                                // member streaming plaintext into a keyed call.
                                None => continue,
                            },
                            None => data.to_vec(), // no group key at all → true legacy/verse, plaintext
                        }
                    }
                };
                let mut bufs = crate::lock_safe(peer_bufs());
                let buf = bufs.entry(id).or_default();
                for b in ulaw.iter() {
                    buf.push_back(ulaw_to_linear(*b));
                }
                while buf.len() > JITTER_CAP {
                    buf.pop_front();
                }
            }
            Err(_) => break, // connection closed / errored
        }
    }
    // Connection ended — drop this peer from the mesh ONLY if we're still the live conn.
    // A superseded re-dial (conn-A) must NOT remove conn-B's live entry, or the call goes
    // silent mid-stream. Guard on stable_id (mirror video.rs).
    if GEN.load(Ordering::SeqCst) == g {
        let mut p = crate::lock_safe(peers());
        if p.get(&id).map(|c| c.stable_id()) == Some(sid) {
            p.remove(&id);
            drop(p);
            crate::lock_safe(peer_bufs()).remove(&id);
        }
    }
}

/// Encode + broadcast one captured PCM frame (16-bit LE) as a μ-law datagram to every peer.
/// No-op when muted or before any connection is up. Pure sync — called from the audio thread.
pub fn send_pcm(pcm_le: &[u8]) {
    if MUTED.load(Ordering::Relaxed) {
        return;
    }
    let conns: Vec<Connection> = crate::lock_safe(peers()).values().cloned().collect();
    if conns.is_empty() {
        return;
    }
    let mut ulaw = Vec::with_capacity(pcm_le.len() / 2);
    for ch in pcm_le.chunks_exact(2) {
        ulaw.push(linear_to_ulaw(i16::from_le_bytes([ch[0], ch[1]])));
    }
    // E2E: seal the frame with the per-call key when set (1:1); else, if a per-call GROUP key is
    // installed, ALWAYS seal with it — FAIL CLOSED. Once this call is E2E-capable (we hold the
    // group key) we never emit plaintext, so a member self-asserting "not media-capable" can no
    // longer downgrade the whole room: an un-keyed member is simply unheard by key-holders, never
    // fed plaintext. Only a call with NO group key at all (true legacy / verse) broadcasts plaintext.
    let bytes = {
        let guard = crate::lock_safe(media());
        if let Some(k) = guard.as_ref() {
            let ctr = MEDIA_TX_CTR.fetch_add(1, Ordering::SeqCst);
            Bytes::from(hey_core::crypto::media_seal(&k.tx, ctr, &ulaw))
        } else {
            drop(guard);
            let g = crate::lock_safe(group_media());
            if let Some(gk) = g.as_ref() {
                let ctr = GROUP_MEDIA_TX_CTR.fetch_add(1, Ordering::SeqCst);
                Bytes::from(hey_core::crypto::media_group_seal(&gk.key, gk.salt, ctr, &ulaw))
            } else {
                Bytes::from(ulaw)
            }
        }
    };
    for c in conns {
        let _ = c.send_datagram(bytes.clone());
    }
}

/// Pull up to `max_bytes` of decoded PCM (16-bit LE) for playback — the **mix** of all peers'
/// jitter buffers (sum + clamp). Peers with fewer buffered samples contribute silence for the tail.
pub fn recv_pcm(max_bytes: usize) -> Vec<u8> {
    let n = max_bytes / 2;
    if n == 0 {
        return Vec::new();
    }
    let mut bufs = crate::lock_safe(peer_bufs());
    if bufs.is_empty() {
        return Vec::new();
    }
    let avail = bufs.values().map(|b| b.len()).max().unwrap_or(0).min(n);
    let mut out = Vec::with_capacity(avail * 2);
    for _ in 0..avail {
        let mut acc: i32 = 0;
        for b in bufs.values_mut() {
            if let Some(s) = b.pop_front() {
                acc += s as i32;
            }
        }
        let s = acc.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

pub fn set_muted(m: bool) {
    MUTED.store(m, Ordering::Relaxed);
}

/// End the session: invalidate recv loops, close + drop every connection, clear buffers.
pub fn stop() {
    reset_session();
}

// ── carrier Router protocol handler (accept inbound voice connections) ──
#[derive(Debug, Clone)]
pub struct VoiceProtocol;

impl iroh::protocol::ProtocolHandler for VoiceProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), iroh::protocol::AcceptError> {
        let g = GEN.load(Ordering::SeqCst);
        bind(conn, g).await;
        Ok(())
    }
}
