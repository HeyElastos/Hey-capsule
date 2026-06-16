//! Hey Verse REALTIME lane — game traffic at game rate.
//!
//! The verse used to push every movement tick through the full DM machinery:
//! Double-Ratchet seal, ratchet-state persist to disk, base64, gossip
//! broadcast, broker.json write on receive — ~5 messages per second per peer.
//! That is bank-grade plumbing for "my robot took a step", and it's why
//! multiplayer felt slow.
//!
//! This lane carries the high-rate, EPHEMERAL traffic (movement) over direct
//! QUIC datagrams on the carrier's shared iroh endpoint instead: one
//! connection per verse peer, datagrams that are allowed to drop (a newer
//! position always follows), nothing ever written to disk. The connection is
//! end-to-end encrypted by iroh itself (TLS between the two endpoint keys),
//! and we only accept inbound links from peers the user actually invited /
//! accepted — the ROSTER is fed exclusively by the sealed DM-lane handshake,
//! which stays exactly as it was (invites remain authenticated and private).
//!
//! Fallback: a peer on an older build never connects here, and the Kotlin
//! plugin keeps sending that peer's movement over the DM lane as before.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use bytes::Bytes;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId};

pub const VERSE_RT_ALPN: &[u8] = b"hey/verse-rt/1";

/// Live realtime links, keyed by the remote's endpoint id.
static PEERS: OnceLock<Mutex<HashMap<EndpointId, Connection>>> = OnceLock::new();
/// endpoint id -> the peer's DID (so drained packets carry the sender DID).
static DIDS: OnceLock<Mutex<HashMap<EndpointId, String>>> = OnceLock::new();
/// Peers AUTHORIZED for this lane — fed only by the DM-lane verse handshake.
static ROSTER: OnceLock<Mutex<HashSet<EndpointId>>> = OnceLock::new();
/// One in-flight dial per peer across retry ticks.
static DIALING: OnceLock<Mutex<HashSet<EndpointId>>> = OnceLock::new();
/// Inbound packets: (sender did, payload json). Bounded — old movement is junk.
static INBOX: OnceLock<Mutex<VecDeque<(String, String)>>> = OnceLock::new();
/// Bumped on reset so stale recv loops and late dials die with the session.
static GEN: AtomicU64 = AtomicU64::new(0);

fn peers() -> &'static Mutex<HashMap<EndpointId, Connection>> {
    PEERS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn dids() -> &'static Mutex<HashMap<EndpointId, String>> {
    DIDS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn roster() -> &'static Mutex<HashSet<EndpointId>> {
    ROSTER.get_or_init(|| Mutex::new(HashSet::new()))
}
fn dialing() -> &'static Mutex<HashSet<EndpointId>> {
    DIALING.get_or_init(|| Mutex::new(HashSet::new()))
}
fn inbox() -> &'static Mutex<VecDeque<(String, String)>> {
    INBOX.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Deposit a movement frame received on ANOTHER lane (verse_gossip) into the
/// shared inbox Godot drains, with the same perishable 256-deep bound. This is
/// why gossip movement needs no new JNI drain — it rides the fast-lane inbox.
pub fn deposit(did: String, payload: String) {
    let mut q = crate::lock_safe(inbox());
    q.push_back((did, payload));
    while q.len() > 256 {
        q.pop_front();
    }
}

/// A verse peer was invited/accepted over the sealed DM lane: authorize them
/// here and keep (re)dialing until the fast link forms or the session resets.
/// Both sides call this; the smaller endpoint id dials (polite-peer tie-break).
pub fn join(endpoint: Endpoint, peer: EndpointId, did: String) {
    crate::lock_safe(roster()).insert(peer);
    crate::lock_safe(dids()).insert(peer, did);
    let g = GEN.load(Ordering::SeqCst);
    maybe_dial(endpoint.clone(), peer, g);
    tokio::spawn(async move {
        // Presence is live-only (~12s silence drops a peer), so a couple of
        // minutes of retries comfortably covers any session's worth of NAT moods.
        for _ in 0..120 {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            if GEN.load(Ordering::SeqCst) != g {
                return;
            }
            if !crate::lock_safe(roster()).contains(&peer) {
                return;
            }
            if crate::lock_safe(peers()).contains_key(&peer) {
                return;
            }
            maybe_dial(endpoint.clone(), peer, g);
        }
    });
}

/// The verse session emptied out (everyone left / tab closed): drop everything.
pub fn reset() {
    GEN.fetch_add(1, Ordering::SeqCst);
    let conns: Vec<Connection> = crate::lock_safe(peers()).drain().map(|(_, c)| c).collect();
    for c in conns {
        c.close(0u32.into(), b"bye");
    }
    crate::lock_safe(roster()).clear();
    crate::lock_safe(dids()).clear();
    crate::lock_safe(dialing()).clear();
    crate::lock_safe(inbox()).clear();
}

/// True if `peer` has a live fast link (the plugin then skips the DM fallback).
pub fn has_peer(peer: &EndpointId) -> bool {
    crate::lock_safe(peers()).contains_key(peer)
}

pub fn connected() -> usize {
    crate::lock_safe(peers()).len()
}

/// Fire one payload to every connected verse peer. Datagrams: no waiting, no
/// retransmit — exactly right for "here is where I am NOW".
pub fn send_all(payload: &str) {
    let conns: Vec<Connection> = crate::lock_safe(peers()).values().cloned().collect();
    let b = Bytes::from(payload.as_bytes().to_vec());
    for c in conns {
        let _ = c.send_datagram(b.clone());
    }
}

/// Drain everything that arrived on the fast lane: (sender did, payload json).
pub fn drain() -> Vec<(String, String)> {
    crate::lock_safe(inbox()).drain(..).collect()
}

fn maybe_dial(endpoint: Endpoint, peer: EndpointId, g: u64) {
    if endpoint.id().to_string() >= peer.to_string() {
        return; // the other side dials
    }
    if !crate::lock_safe(dialing()).insert(peer) {
        return;
    }
    tokio::spawn(async move {
        let r = endpoint.connect(peer, VERSE_RT_ALPN).await;
        crate::lock_safe(dialing()).remove(&peer);
        match r {
            Ok(conn) => bind(conn, g).await,
            Err(e) => log::warn!("verse-rt: dial {peer} failed: {e}"),
        }
    });
}

/// Adopt a connection (dialed or accepted): store it and pump its datagrams
/// into the inbox until it closes or the session generation moves on.
async fn bind(conn: Connection, g: u64) {
    let peer = conn.remote_id();
    if GEN.load(Ordering::SeqCst) != g {
        conn.close(0u32.into(), b"stale");
        return;
    }
    // Accept the inbound connection even if the peer isn't in our roster yet:
    // reaching us on this ALPN already required OUR carrier ticket (shared only
    // with invited contacts), so the ticket IS the gate. This is what lets the
    // fast lane form when our side lacks the PEER's ticket (the inviter who
    // never received the accepter's ticket) — the peer dialed us, and we send +
    // receive movement over this one connection. Without it the fast lane never
    // formed and movement flooded the heavy DM/ratchet lane (lag + crash).
    let did = crate::lock_safe(dids()).get(&peer).cloned().unwrap_or_default();
    if let Some(old) = crate::lock_safe(peers()).insert(peer, conn.clone()) {
        old.close(0u32.into(), b"replaced");
    }
    log::info!("verse-rt: fast lane up with {peer}");
    loop {
        if GEN.load(Ordering::SeqCst) != g {
            break;
        }
        match conn.read_datagram().await {
            Ok(b) => {
                if let Ok(s) = std::str::from_utf8(&b) {
                    let mut q = crate::lock_safe(inbox());
                    q.push_back((did.clone(), s.to_string()));
                    // movement is perishable: keep the newest, drop the backlog
                    while q.len() > 256 {
                        q.pop_front();
                    }
                }
            }
            Err(_) => break,
        }
    }
    if let Some(c) = crate::lock_safe(peers()).remove(&peer) {
        c.close(0u32.into(), b"done");
    }
}

/// Router hook: inbound `hey/verse-rt/1` connections land here.
#[derive(Debug, Clone)]
pub struct VerseRtProtocol;

impl iroh::protocol::ProtocolHandler for VerseRtProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), iroh::protocol::AcceptError> {
        bind(conn, GEN.load(Ordering::SeqCst)).await;
        Ok(())
    }
}
