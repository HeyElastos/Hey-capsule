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
/// not already connected. Existing peers are left alone; a peer that drops off the list is NOT torn
/// down here (its connection closing handles cleanup) so a transient roster gap can't kill audio.
pub fn sync_peers(endpoint: Endpoint, wanted: Vec<EndpointId>) {
    let g = GEN.load(Ordering::SeqCst);
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
            Err(e) => log::warn!("voice: dial {peer} failed: {e}"),
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
                let mut bufs = crate::lock_safe(peer_bufs());
                let buf = bufs.entry(id).or_default();
                for b in data.iter() {
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
    let bytes = Bytes::from(ulaw);
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
