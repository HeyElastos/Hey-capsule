// Outbox + retry queue for DM publishes.
//
// `peer::publish` can fail for transient reasons (network glitch, the
// peer provider being unreachable, a 5xx). Today we'd drop the failure
// on the floor (`let _ = peer::publish(...).await`) and the message
// would never reach the peer — the local conversation already has it,
// so the sender never sees a problem; the recipient just never gets
// the message.
//
// The outbox closes that gap. Every publish attempt that errors gets
// stashed here as a serialized wire string + the topic + the
// pseudonymous sender_id, with an exponential-backoff retry schedule.
// `flush()` walks the queue once per peer_receiver poll cycle and
// retries each item whose `next_attempt_ms` has elapsed. Successful
// publish → the item is dropped. Repeated failure → backoff doubles
// up to a cap; after ATTEMPTS_MAX retries the item is dropped with a
// console warning.
//
// Storage: `Hey/dm/outbox.json` as `Vec<OutboxItem>`. The whole queue
// is rewritten on each modification (cap at 1000 items so the JSON
// stays bounded). For Hey-scale chat traffic that's plenty.

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::api::frag;
use crate::runtime::{peer, storage};

const OUTBOX_FILE: &str = "dm/outbox.json";
const MAX_ITEMS: usize = 1000;
const ATTEMPTS_MAX: u32 = 12;
/// Initial backoff before the first retry, in ms. Subsequent retries
/// double up to BACKOFF_CAP_MS.
const BACKOFF_INITIAL_MS: i64 = 1_500;  // first retry fast (re-mesh is ~1-2s now); then doubles
/// Cap retry delay at 1 hour. Beyond ATTEMPTS_MAX we drop the item.
const BACKOFF_CAP_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxItem {
    pub id: String,
    pub topic: String,
    pub sender_id: String,
    pub ts: i64,
    pub wire: String,
    #[serde(default)]
    pub retries: u32,
    #[serde(default)]
    pub next_attempt_ms: i64,
    /// Peer node ticket(s) to (re)graft the gossip mesh to before a retry, so
    /// `flush` re-forms a decayed/never-formed topic neighbor instead of
    /// re-broadcasting into an empty active_view (a silent no-op). Empty for
    /// same-runtime sends. `default` keeps older queued items deserializable.
    #[serde(default)]
    pub boot: Vec<String>,
}

fn now_ms() -> i64 {
    crate::plat::now_ms()
}

/// Process-global async gate serializing the read-modify-write of
/// `dm/outbox.json`. On NATIVE the engine runs across multiple OS threads (the
/// peer_receiver poll thread calls `flush()`; JNI threads call `enqueue()` /
/// `purge_topic()`), so two unsynchronized read→modify→write cycles can
/// interleave: a concurrently-enqueued item gets clobbered by a stale write, or
/// a just-delivered item gets resurrected. This is the lost-update race. The
/// gate makes every read-modify-write atomic with respect to the others. It is
/// an async (atomic-based, no-OS-thread) mutex so it works on single-threaded
/// wasm too, where it's an uncontended no-op.
fn outbox_gate() -> &'static futures_util::lock::Mutex<()> {
    static G: std::sync::OnceLock<futures_util::lock::Mutex<()>> = std::sync::OnceLock::new();
    G.get_or_init(|| futures_util::lock::Mutex::new(()))
}

/// Timing-jitter window (ms). A small random pre-send delay so an on-path
/// observer can't use precise send TIMING to correlate or fingerprint traffic —
/// the timing-side companion to the `hpq-2` size-bucket padding in `crypto`
/// (which already hides message length). Sub-300ms is imperceptible in chat.
/// Applied once per wire (not per fragment); verse real-time movement rides a
/// different lane (verse_rt / verse_gossip) and is unaffected.
const JITTER_MIN_MS: i32 = 15;
const JITTER_SPAN_MS: u32 = 260; // effective range ~15..=275 ms

fn jitter_ms() -> i32 {
    JITTER_MIN_MS + (OsRng.next_u32() % JITTER_SPAN_MS) as i32
}

/// True when a `gossip_send` response says the broadcast reached NO remote peer.
/// carrier emits `{status:ok, broadcast:"local_only"}` only when the underlying
/// `broadcast()` errors. A bare `{status:ok}` is treated as delivered: the
/// 0-neighbor SILENT no-op (which also returns bare ok) is prevented upstream by
/// `join_topic_with`'s neighbor gate before we ever publish.
fn says_local_only(v: &serde_json::Value) -> bool {
    v.get("broadcast")
        .or_else(|| v.get("data").and_then(|d| d.get("broadcast")))
        .and_then(|b| b.as_str())
        == Some("local_only")
}

/// Publish a wire, fragmenting it transparently when it exceeds the gossip
/// size cap (iroh-gossip drops messages over ~4096 B; the PQ handshake is
/// ~23 KB). Every fragment is its own `gossip_send`; the receiver reassembles
/// via `frag::reassemble` before `receive_v2_wire`. Returns true only if EVERY
/// fragment send succeeded and none was a local-only (0-neighbor) broadcast.
/// A wire that already fits is the `n == 1` case — identical to a bare publish.
async fn publish_wire(topic: &str, sender_id: &str, wire: &str, ts: i64) -> bool {
    // Timing-jitter: a small random delay before sending so precise send timing
    // can't be used to correlate/fingerprint traffic (companion to the size
    // padding). Once per wire, before the fragment loop, so fragments still go
    // back-to-back (they reassemble into one logical message anyway).
    crate::plat::sleep_ms(jitter_ms()).await;
    for f in frag::fragment(wire) {
        let res = peer::publish(peer::PublishArgs {
            topic,
            message: &f,
            sender_id,
            ts,
            signature: "v2-sealed",
        })
        .await;
        if !matches!(&res, Ok(v) if !says_local_only(v)) {
            return false;
        }
    }
    true
}

fn backoff_for(retries: u32) -> i64 {
    let raw = BACKOFF_INITIAL_MS.saturating_mul(2_i64.saturating_pow(retries));
    raw.min(BACKOFF_CAP_MS)
}

async fn read_items() -> Vec<OutboxItem> {
    storage::read_json(OUTBOX_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

async fn write_items(items: &[OutboxItem]) {
    let v = match serde_json::to_value(items) {
        Ok(v) => v,
        Err(_) => return,
    };
    let _ = storage::write_json(OUTBOX_FILE, &v).await;
}

/// Stash a publish that has already failed. `next_attempt_ms` is
/// scheduled immediately so the next `flush()` will retry; if that
/// retry also fails the queue's own backoff takes over.
pub async fn enqueue(topic: &str, boot: &[String], sender_id: &str, wire: &str) {
    // Hold the gate across the whole read+write so a concurrent flush() /
    // enqueue() / purge_topic() can't clobber this insertion (lost-update race).
    let _g = outbox_gate().lock().await;
    let mut items = read_items().await;
    if items.len() >= MAX_ITEMS {
        // Drop the oldest to make room. Better than refusing the newest.
        items.remove(0);
    }
    items.push(OutboxItem {
        id: uuid::Uuid::new_v4().to_string(),
        topic: topic.into(),
        sender_id: sender_id.into(),
        ts: now_ms(),
        wire: wire.into(),
        retries: 0,
        next_attempt_ms: now_ms(),
        boot: boot.to_vec(),
    });
    write_items(&items).await;
}

/// Publish once, and queue for retry unless delivery is CONFIRMED. Callers must
/// have already `join_topic_with(topic, boot)`'d (which gates on a neighbor);
/// `boot` is threaded here only so a retry in `flush` can re-graft. Delivery is
/// confirmed iff the call succeeded, wasn't a `local_only` broadcast, AND the
/// topic currently has a gossip neighbor — because carrier returns bare
/// {status:ok} for a 0-neighbor no-op too, so the response alone can't be
/// trusted. Returns Ok only when confirmed delivered.
pub async fn publish_or_enqueue(
    topic: &str,
    boot: &[String],
    sender_id: &str,
    wire: &str,
) -> Result<(), String> {
    // Wire size matters: iroh-gossip silently drops messages over its
    // max_message_size (4096 B). The PQ handshake envelope measured 23,454 B,
    // so it never crossed even with a healthy neighbor — until publish_wire
    // (below) fragments it. Surface the size for triage.
    crate::plat::debug(&format!(
        "publish_or_enqueue: topic={topic} wire={} bytes boot={}",
        wire.len(),
        boot.len()
    ));
    // Fragment + send (the "v2-sealed" outer signature placeholder lives in
    // publish_wire; the real signature is inside the ChaCha20-Poly1305 envelope,
    // not on the outer wire). ok_send is true only if every fragment crossed.
    let ok_send = publish_wire(topic, sender_id, wire, now_ms()).await;
    // A bare ok is NOT proof of REMOTE delivery (the 0-neighbor no-op returns it
    // too): when we expect a remote peer (boot non-empty) require an actual
    // topic neighbor, else the broadcast reached nobody => queue for retry. When
    // no remote peer is expected (boot empty: same-runtime / legacy bare-did),
    // carrier's local buffer delivers to the co-resident recipient, so a bare ok
    // IS delivery — don't queue those for a retry that can never confirm a peer.
    let expect_remote = boot.iter().any(|t| !t.is_empty());
    let delivered = ok_send && (!expect_remote || peer::has_topic_peer(topic).await);
    if !delivered {
        enqueue(topic, boot, sender_id, wire).await;
        return Err("publish not confirmed delivered; queued for retry".into());
    }
    Ok(())
}

/// Walk the outbox and retry items whose `next_attempt_ms` has elapsed.
/// Called from peer_receiver::poll_once each cycle.
///
/// Three phases so the slow network retries DON'T hold the gate (which would
/// stall every enqueue() for the whole retry budget) while still being safe
/// against the lost-update race:
///   1. take gate, snapshot the queue, drop gate;
///   2. lock-free: attempt delivery of each due item, recording delivered ids,
///      dropped (max-attempts) ids, and per-id retry/next_attempt updates;
///   3. take gate, RE-READ the on-disk queue, drop the delivered+dropped ids,
///      apply retry updates ONLY to ids still present, leave any items that
///      were concurrently ENQUEUED while we were retrying untouched, write,
///      drop gate.
/// Merging by `item.id` (rather than overwriting with the phase-1 snapshot)
/// preserves concurrent enqueues and never resurrects a delivered item.
pub async fn flush() {
    // Phase 1 — snapshot under the gate, then release it for the network work.
    let snapshot: Vec<OutboxItem> = {
        let _g = outbox_gate().lock().await;
        read_items().await
    };
    if snapshot.is_empty() {
        return;
    }
    let now = now_ms();

    // Phase 2 — lock-free delivery attempts. Collect per-id outcomes; touch no
    // shared state. `done` = ids to remove (delivered or dropped at max
    // attempts); `retry_updates` = (id, retries, next_attempt_ms) for items to
    // re-arm IF they still exist on disk at phase 3.
    use std::collections::{HashMap, HashSet};
    let mut done: HashSet<String> = HashSet::new();
    let mut retry_updates: HashMap<String, (u32, i64)> = HashMap::new();
    for item in &snapshot {
        if item.next_attempt_ms > now {
            continue; // not yet due — leave on disk unchanged
        }
        // Re-form the topic neighbor BEFORE re-broadcasting — a retry into an
        // empty active_view is the same silent no-op we're guarding against,
        // just on the retry path. join_topic_with re-dials item.boot and waits
        // for NeighborUp; with no boot (same-runtime) it's a cheap no-op.
        let _ = peer::join_topic_with(&item.topic, &item.boot).await;
        let ok_send = publish_wire(&item.topic, &item.sender_id, &item.wire, item.ts).await;
        let expect_remote = item.boot.iter().any(|t| !t.is_empty());
        if ok_send && (!expect_remote || peer::has_topic_peer(&item.topic).await) {
            // delivered (sent AND a neighbor exists, or same-runtime where the
            // local buffer is sufficient) — mark for removal.
            done.insert(item.id.clone());
            continue;
        }
        let retries = item.retries + 1;
        if retries >= ATTEMPTS_MAX {
            crate::plat::warn(&format!(
                "[hey-core] outbox: dropping item {} on topic {} after {} attempts",
                item.id, item.topic, retries
            ));
            done.insert(item.id.clone());
            continue;
        }
        retry_updates.insert(item.id.clone(), (retries, now + backoff_for(retries)));
    }

    if done.is_empty() && retry_updates.is_empty() {
        return; // nothing changed (all items were not-yet-due)
    }

    // Phase 3 — re-read under the gate and MERGE by id. Anything enqueued while
    // we were retrying (a new id not in our snapshot) is left untouched; a
    // delivered/dropped id is removed; a still-present retry id is re-armed.
    let _g = outbox_gate().lock().await;
    let mut current = read_items().await;
    let mut changed = false;
    current.retain(|it| {
        if done.contains(&it.id) {
            changed = true;
            return false;
        }
        true
    });
    for it in current.iter_mut() {
        if let Some((retries, next_attempt_ms)) = retry_updates.get(&it.id) {
            it.retries = *retries;
            it.next_attempt_ms = *next_attempt_ms;
            changed = true;
        }
    }
    if changed {
        write_items(&current).await;
    }
}

/// How many items are awaiting retry. Cheap (one storage read). Useful
/// for surfacing a "N messages queued" badge in the UI.
pub async fn pending_count() -> usize {
    read_items().await.len()
}

/// Hard reset — used by session::wipe(). Drops every queued message
/// without trying to send.
pub async fn clear() {
    let _ = storage::remove(OUTBOX_FILE).await;
}

/// Drop any items whose topic matches `prefix` exactly or starts with
/// `prefix/`. Used when queue rotation makes a topic obsolete.
pub async fn purge_topic(topic: &str) {
    // Hold the gate across read+write so a concurrent flush()/enqueue() can't
    // clobber this purge or be clobbered by it (lost-update race).
    let _g = outbox_gate().lock().await;
    let items = read_items().await;
    let kept: Vec<OutboxItem> = items.into_iter().filter(|i| i.topic != topic).collect();
    write_items(&kept).await;
}

/// Self-introspection: serialize a synthetic OutboxItem roundtrip. Used
/// by dms::self_test_v2 to confirm the storage shape works after
/// schema changes.
#[allow(dead_code)]
pub fn schema_roundtrip_ok() -> bool {
    let item = OutboxItem {
        id: "test".into(),
        topic: "q/abc".into(),
        sender_id: "deadbeef".into(),
        ts: 1,
        wire: r#"{"type":"dm.v2","envelope":{}}"#.into(),
        retries: 0,
        next_attempt_ms: 0,
        boot: Vec::new(),
    };
    let v = match serde_json::to_value(&item) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let back: Result<OutboxItem, _> = serde_json::from_value(v);
    back.is_ok()
}
