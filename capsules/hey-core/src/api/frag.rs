//! Transparent gossip fragmentation for oversized DM wires.
//!
//! iroh-gossip silently drops any message over its `max_message_size`
//! (4096 B default; the runtime carrier sets no override — confirmed in
//! carrier.rs `Gossip::builder().spawn(..)`). The PQ invite **handshake**
//! envelope runs ~23 KB (ML-KEM ct + ML-KEM pubkey + ratchet bootstrap, all
//! base64 inside a ChaCha envelope), so it NEVER crosses — `gossip_send`
//! still returns `{status:ok}`, the neighbor is present, and the outbox marks
//! it delivered, yet the peer's `gossip_recv` stays empty forever. That single
//! fact is the long-hunted cross-runtime "send ok / recv empty" bug.
//!
//! This module is the vanilla-upstream-safe fix (no runtime patch): split an
//! oversized wire into ordered fragments small enough to pass the cap, tag
//! each with a shared message id, and reassemble on the receive side BEFORE
//! the wire reaches `dms::receive_v2_wire`. Small wires (the common short
//! post-ratchet text) pass through byte-for-byte unchanged, so this is a pure
//! superset of the existing format. Both ends must run this code; a fragment
//! sent to an old peer simply fails its dm.v2 parse and is dropped (no worse
//! than today, where the whole oversized message was dropped anyway).

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

/// Max bytes of ORIGINAL wire carried per fragment. The carrier wraps each
/// gossip message in a small struct (sender_id/nick/nonce/ts ~120 B) and the
/// fragment header adds ~80 B, so 3000 leaves a wide margin under the 4096 cap
/// (empirically ~3950 B of content crosses; 4000+ does not).
const CHUNK_BYTES: usize = 3000;
/// Drop partial reassemblies older than this. A lost fragment must not pin
/// memory forever; the outbox re-sends the whole wire on its own schedule.
const REASSEMBLY_TTL_MS: i64 = 120_000;
/// Hard caps so a remote peer can't drive unbounded memory growth (SB-4) by
/// claiming a huge fragment count, an out-of-range index, an oversized chunk,
/// or by flooding distinct message ids between TTL sweeps. The real handshake
/// is ~8 fragments and inline attachments a few more; 128 * CHUNK_BYTES (~384 KB
/// of original wire) is a generous ceiling — anything larger rides
/// iroh-blobs/IPFS, not gossip. Worst-case buffered memory is therefore bounded
/// by MAX_FRAGS * CHUNK_BYTES * MAX_PARTIALS (~6 MB), not the attacker's input.
const MAX_FRAGS: u32 = 128;
const MAX_PARTIALS: usize = 16;
/// Wire discriminator. Bump the suffix if the fragment shape ever changes.
const TAG: &str = "hcfrag1";

#[derive(Serialize, Deserialize)]
struct Fragment {
    /// Always == `TAG`; lets the receiver tell a fragment from a plain wire.
    t: String,
    /// Per-message id (uuid v4) shared by every fragment of one wire.
    id: String,
    /// Fragment index in `0..n`.
    i: u32,
    /// Total fragment count.
    n: u32,
    /// A `CHUNK_BYTES`-sized slice of the original wire (ASCII JSON/base64).
    d: String,
}

/// True if this wire must be fragmented to cross the gossip size cap.
pub fn needs_fragment(wire: &str) -> bool {
    wire.len() > CHUNK_BYTES
}

/// Split `wire` into ordered fragment JSON strings — each its own gossip
/// message. A wire that already fits is returned as a single untouched element
/// (so callers can always iterate the result without special-casing).
pub fn fragment(wire: &str) -> Vec<String> {
    if !needs_fragment(wire) {
        return vec![wire.to_string()];
    }
    let len = wire.len();
    let n = len.div_ceil(CHUNK_BYTES) as u32;
    let id = uuid::Uuid::new_v4().to_string();
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let start = i as usize * CHUNK_BYTES;
        let end = (start + CHUNK_BYTES).min(len);
        // The wire is JSON containing base64 — pure ASCII — so byte-index
        // slicing never splits a multi-byte char.
        let frag = Fragment {
            t: TAG.to_string(),
            id: id.clone(),
            i,
            n,
            d: wire[start..end].to_string(),
        };
        out.push(serde_json::to_string(&frag).unwrap_or_default());
    }
    out
}

thread_local! {
    static BUF: RefCell<HashMap<String, Partial>> = RefCell::new(HashMap::new());
}

struct Partial {
    n: u32,
    parts: HashMap<u32, String>,
    first_ms: i64,
}

/// Feed a received gossip `content`. Returns:
///  * `Some(wire)` — a complete wire is ready: either `content` was a plain
///    (non-fragment) wire passed straight through, or this was the fragment
///    that completed a set;
///  * `None` — a fragment was buffered and the set is still incomplete.
pub fn reassemble(content: &str) -> Option<String> {
    // A plain wire won't deserialize into Fragment (all 5 fields required), so
    // the common small-message path returns immediately, untouched.
    let frag: Fragment = match serde_json::from_str(content) {
        Ok(f) => f,
        Err(_) => return Some(content.to_string()),
    };
    if frag.t != TAG {
        return Some(content.to_string());
    }
    // Bound attacker-controlled fields BEFORE buffering (SB-4 remote-DoS guard):
    // a zero/oversized count, an index outside 0..n, or a chunk larger than the
    // producer ever emits is never legitimate — drop it instead of allocating.
    if frag.n == 0 || frag.n > MAX_FRAGS || frag.i >= frag.n || frag.d.len() > CHUNK_BYTES {
        return None;
    }
    let now = crate::plat::now_ms();
    BUF.with(|b| {
        let mut m = b.borrow_mut();
        m.retain(|_, p| now - p.first_ms < REASSEMBLY_TTL_MS);
        // Cap concurrent partial reassemblies; if a flood of distinct ids tries
        // to pin memory between TTL sweeps, evict the oldest by first_ms.
        if m.len() >= MAX_PARTIALS && !m.contains_key(&frag.id) {
            if let Some(oldest) = m.iter().min_by_key(|(_, p)| p.first_ms).map(|(k, _)| k.clone()) {
                m.remove(&oldest);
            }
        }
        let p = m.entry(frag.id.clone()).or_insert_with(|| Partial {
            n: frag.n,
            parts: HashMap::new(),
            first_ms: now,
        });
        // A fragment whose count disagrees with the set's established n is
        // hostile/corrupt — ignore it rather than letting it resize the set.
        if frag.n != p.n {
            return None;
        }
        p.parts.insert(frag.i, frag.d);
        if p.parts.len() as u32 != p.n {
            return None;
        }
        // Complete — join the chunks in index order.
        let mut wire = String::new();
        for i in 0..p.n {
            match p.parts.get(&i) {
                Some(c) => wire.push_str(c),
                // A duplicate-completed map can't have a gap here, but guard.
                None => return None,
            }
        }
        m.remove(&frag.id);
        Some(wire)
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn big(n: usize) -> String {
        // ASCII payload that looks like a real wire (JSON + base64-ish).
        let mut s = String::from("{\"type\":\"dm.v2\",\"envelope\":\"");
        while s.len() < n {
            s.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/");
        }
        s.truncate(n);
        s.push_str("\"}");
        s
    }

    #[test]
    fn small_wire_passes_through_unfragmented() {
        let w = big(500);
        assert!(!needs_fragment(&w));
        assert_eq!(fragment(&w), vec![w.clone()]);
        assert_eq!(reassemble(&w), Some(w));
    }

    #[test]
    fn handshake_sized_wire_round_trips() {
        // The real handshake measured 23,454 bytes on the wire.
        let w = big(23_454);
        let frags = fragment(&w);
        assert!(frags.len() >= 8, "expected many fragments, got {}", frags.len());
        // Every fragment must fit the gossip cap with margin.
        for f in &frags {
            assert!(f.len() < 3950, "fragment too big: {}", f.len());
        }
        // Feed all-but-last: still incomplete.
        let mut out = None;
        for (i, f) in frags.iter().enumerate() {
            let r = reassemble(f);
            if i < frags.len() - 1 {
                assert!(r.is_none(), "completed early at {i}");
            } else {
                out = r;
            }
        }
        assert_eq!(out.as_deref(), Some(w.as_str()));
    }

    #[test]
    fn out_of_order_fragments_reassemble() {
        let w = big(10_000);
        let mut frags = fragment(&w);
        frags.reverse();
        let mut out = None;
        for f in &frags {
            if let Some(r) = reassemble(f) {
                out = Some(r);
            }
        }
        assert_eq!(out.as_deref(), Some(w.as_str()));
    }

    #[test]
    fn malicious_fragments_are_rejected_without_buffering() {
        // SB-4: a hostile fragment must be dropped (None) and must not pin memory.
        let oversized_n = serde_json::to_string(&Fragment {
            t: TAG.to_string(),
            id: "atk-n".to_string(),
            i: 0,
            n: MAX_FRAGS + 1,
            d: "x".to_string(),
        })
        .unwrap();
        assert_eq!(reassemble(&oversized_n), None);

        let bad_index = serde_json::to_string(&Fragment {
            t: TAG.to_string(),
            id: "atk-i".to_string(),
            i: 5,
            n: 3,
            d: "x".to_string(),
        })
        .unwrap();
        assert_eq!(reassemble(&bad_index), None);

        let zero_n = serde_json::to_string(&Fragment {
            t: TAG.to_string(),
            id: "atk-0".to_string(),
            i: 0,
            n: 0,
            d: "x".to_string(),
        })
        .unwrap();
        assert_eq!(reassemble(&zero_n), None);

        let fat_chunk = serde_json::to_string(&Fragment {
            t: TAG.to_string(),
            id: "atk-fat".to_string(),
            i: 0,
            n: 1,
            d: "z".repeat(CHUNK_BYTES + 1),
        })
        .unwrap();
        assert_eq!(reassemble(&fat_chunk), None);

        // None of the above should have created a buffered partial.
        BUF.with(|b| {
            let m = b.borrow();
            for k in ["atk-n", "atk-i", "atk-0", "atk-fat"] {
                assert!(!m.contains_key(k), "hostile id {k} must not be buffered");
            }
        });

        // A well-formed fragment set still reassembles after the rejections.
        let w = big(8_000);
        let frags = fragment(&w);
        let mut out = None;
        for f in &frags {
            if let Some(r) = reassemble(f) {
                out = Some(r);
            }
        }
        assert_eq!(out.as_deref(), Some(w.as_str()));
    }

    #[test]
    fn interleaved_messages_dont_cross_contaminate() {
        let a = big(7_000);
        let b = big(9_000);
        let fa = fragment(&a);
        let fb = fragment(&b);
        // Interleave A and B fragments; each id reassembles independently.
        let mut ra = None;
        let mut rb = None;
        let max = fa.len().max(fb.len());
        for i in 0..max {
            if let Some(f) = fa.get(i) {
                if let Some(r) = reassemble(f) {
                    ra = Some(r);
                }
            }
            if let Some(f) = fb.get(i) {
                if let Some(r) = reassemble(f) {
                    rb = Some(r);
                }
            }
        }
        assert_eq!(ra.as_deref(), Some(a.as_str()));
        assert_eq!(rb.as_deref(), Some(b.as_str()));
    }
}
