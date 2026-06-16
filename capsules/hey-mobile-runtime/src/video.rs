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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
}

/// Cumulative frames dropped at the send queue this process — the adaptive loop
/// watches the delta to back the bitrate off (then recover) before the link lags.
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Begin a 1:1 video session with `peer` (a strict 1-peer mesh, mirroring voice).
/// Both sides call this; the smaller EndpointId dials. Must run on the carrier runtime.
pub async fn start(endpoint: Endpoint, peer: EndpointId) {
    reset_session();
    crate::lock_safe(roster()).insert(peer);
    let g = GEN.load(Ordering::SeqCst);
    log::info!("video: session start, peer={peer} (we dial iff {} < {peer})", endpoint.id());
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

/// Number of LIVE video links — the UI's "connecting video…" probe (0 while dialing).
pub fn connected_peers() -> usize {
    crate::lock_safe(peers()).len()
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
            Err(e) => log::warn!("video: dial {peer} failed: {e}"),
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
    if !crate::lock_safe(roster()).contains(&id) {
        log::warn!("video: rejected {id} — not in the call roster yet (will retry)");
        return;
    }
    let sid = conn.stable_id();
    log::info!("video: link UP to {id} (conn {sid})");
    crate::lock_safe(peers()).insert(id, conn.clone());
    crate::lock_safe(inbound()).entry(id).or_default();

    // WRITER: one uni-stream out, length-prefixed frames as they are queued.
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_CAP);
    crate::lock_safe(outbox()).insert(id, tx);
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
                        let len = frame.len() as u32;
                        if send.write_all(&len.to_le_bytes()).await.is_err() {
                            break;
                        }
                        if send.write_all(&frame).await.is_err() {
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
            if len > MAX_FRAME_BYTES {
                break; // malformed / hostile → drop the connection
            }
            if len == 0 {
                continue; // keepalive — the stream is alive even before frames flow
            }
            let mut frame = vec![0u8; len];
            if recv.read_exact(&mut frame).await.is_err() {
                break;
            }
            let mut q = crate::lock_safe(inbound());
            let dq = q.entry(id).or_default();
            dq.push_back(frame);
            while dq.len() > INBOUND_CAP {
                dq.pop_front(); // drop oldest → no growing backlog
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
pub fn recv_frame() -> Vec<u8> {
    let mut q = crate::lock_safe(inbound());
    for dq in q.values_mut() {
        if let Some(f) = dq.pop_front() {
            return f;
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
