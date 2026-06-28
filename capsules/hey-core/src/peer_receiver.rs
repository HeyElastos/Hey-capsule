// Peer-receive loop for the shared chat engine — the DM half of
// hey-social's peer_receiver, with the social routing stripped.
//
// hey-social's version also routed post.create.v2 / post.* / follow.request /
// group.* and read the follows store to subscribe to followed users' post
// topics. None of that belongs in the messenger, so it is gone here. What
// remains is the chat loop: subscribe to the metadata-safe v2 per-pair
// queues, route incoming DM events into the DM store, and flush the outbox
// each cycle.
//
// Topics:
//   * q/<256bit> (per-pair)      — v2 sealed-sender queues (dms::my_v2_topics)
//
// Run as a background task started after sign-in (see the bin crate's boot).
// When hey-social adopts hey-core, route() can be made pluggable to re-add
// its social arms.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use serde_json::Value;

use crate::api::dms;
use crate::api::outbox;
use crate::events::{from_wire_string, verify_signed_event, VerifyResult};
use crate::runtime::peer;
use crate::session;

const POLL_INTERVAL_MS: i32 = 1_000;  // steady-state arrival floor (was 2s)
const RECV_LIMIT: u32 = 50;

// ── Pluggable app routing (hey-social registers its feed/group arms) ──
//
// The engine handles DMs natively via the v2 sealed-sender queues (every app
// gets DMs). An app can ALSO register handlers for its own SignedEvent types
// (hey-social: post.create.v2 / post.* / follow.request / group.*) and provide
// extra topics to subscribe each poll (hey-social: followed-user post topics +
// its follow inbox). hey-chat registers nothing, so its loop is DM-only — the
// behavior is byte-identical to before. Single-threaded wasm ⇒ thread_local +
// Rc is sufficient; register BEFORE `run()`.

type BoxFut = Pin<Box<dyn Future<Output = Result<(), String>>>>;
type RouteHandler = Rc<dyn Fn(String, Value, String) -> BoxFut>;
// Each entry is (topic, bootstrap node tickets). Bootstrap is empty for topics
// we originate (our own posts/follow inbox); for a followed user's posts topic
// it's their node ticket so the gossip mesh forms across runtimes.
type TopicsProvider = Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Vec<(String, Vec<String>)>>>>>;

// Session-scoped cache of topics we've already issued join_topic for —
// join is idempotent provider-side but the round-trip is wasteful. Resets
// on logout (wasm itself resets).
thread_local! {
    static JOINED_TOPICS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static HANDLERS: RefCell<HashMap<String, RouteHandler>> = RefCell::new(HashMap::new());
    static EXTRA_TOPICS: RefCell<Option<TopicsProvider>> = RefCell::new(None);
}

/// Register an app handler for a SignedEvent `event_type`. Called BEFORE `run()`.
/// DMs travel the v2 queue path (not SignedEvents), so they don't go through
/// here. The handler receives owned `(event_type, payload, sender_did)`.
pub fn register_handler<F, Fut>(event_type: &str, handler: F)
where
    F: Fn(String, Value, String) -> Fut + 'static,
    Fut: Future<Output = Result<(), String>> + 'static,
{
    let h: RouteHandler = Rc::new(move |t, p, s| Box::pin(handler(t, p, s)));
    HANDLERS.with(|m| {
        m.borrow_mut().insert(event_type.to_string(), h);
    });
}

/// Register a provider of EXTRA topics to subscribe + drain each poll (all
/// consumed on the SignedEvent path, skipping our own sender). Called BEFORE
/// `run()`. hey-social returns its followed-user post topics + follow inbox.
pub fn set_extra_topics_provider<F, Fut>(f: F)
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Vec<(String, Vec<String>)>> + 'static,
{
    let p: TopicsProvider = Rc::new(move || Box::pin(f()));
    EXTRA_TOPICS.with(|c| {
        *c.borrow_mut() = Some(p);
    });
}

async fn ensure_joined(topic: &str, bootstrap: &[String]) {
    let already = JOINED_TOPICS.with(|s| s.borrow().contains(topic));
    if already {
        return;
    }
    if peer::join_topic_with(topic, bootstrap).await.is_ok() {
        JOINED_TOPICS.with(|s| s.borrow_mut().insert(topic.to_string()));
    }
}

/// Drop a topic from the joined cache + tell the provider to unsubscribe.
/// Used by dms::receive_handshake after queue rotation makes an invite
/// queue single-use.
pub async fn forget_topic(topic: &str) {
    JOINED_TOPICS.with(|s| {
        s.borrow_mut().remove(topic);
    });
    let _ = peer::leave_topic(topic).await;
}

/// Background poll loop. No-op while signed out.
pub async fn run() {
    loop {
        sleep_ms(POLL_INTERVAL_MS).await;
        if let Some(s) = session::current() {
            if let Err(e) = poll_once(&s.did_key).await {
                crate::plat::warn(&format!("[hey-core] peer_receiver poll error: {e}"));
            }
        }
    }
}

async fn poll_once(my_did: &str) -> Result<(), String> {
    // PROCESSING DEFERRED — storage locked (DEK cleared on app-lock) OR seed
    // sealed (vault-ON headless cold start, pre-unlock). Either way the carrier
    // still JOINS every topic, forms neighbors, and BUFFERS inbound wires into
    // broker.json, so the device stays meshed and messages queue. We only skip
    // CONSUME (decrypt/flush): that needs the DEK to read ratchet state AND the
    // seed to decrypt. Everything drains the instant we fully unlock.
    let locked = crate::plat::processing_deferred();
    // Outbox-strand diagnosis (adb-greppable): when processing is deferred (storage DEK
    // cleared on app-lock OR the seed sealed on a vault-ON headless boot) we JOIN + buffer
    // but do NOT consume/flush — so a new-conversation PQ-invite handshake + first DM stay
    // stranded in the outbox until unlock. Log ONLY on the edge (locked⇄unlocked transition)
    // so the 2s poll never spams. The fix is to never STAY deferred: the app re-installs the
    // storage DEK on resume/unlock (HeyApi.installStorageKey) so this clears promptly.
    {
        thread_local! { static LAST_DEFERRED: RefCell<Option<bool>> = const { RefCell::new(None) }; }
        LAST_DEFERRED.with(|c| {
            let mut last = c.borrow_mut();
            if *last != Some(locked) {
                if locked {
                    crate::plat::warn("outbox: deferred (sealed/locked) — not flushing; messages queue until unlock");
                } else if last.is_some() {
                    crate::plat::warn("outbox: unlocked — resuming consume + flush");
                }
                *last = Some(locked);
            }
        });
    }
    let consumer_id = format!("{}:{}", crate::ctx::capsule_id(), my_did);

    // 0. App-provided extra topics (hey-social: followed-user post topics + its
    //    follow inbox). Consumed on the SignedEvent path → app-registered route
    //    handlers. Empty for hey-chat.
    let extra = match EXTRA_TOPICS.with(|c| c.borrow().clone()) {
        Some(p) => p().await,
        None => Vec::new(),
    };
    for (topic, bootstrap) in &extra {
        ensure_joined(topic, bootstrap).await;
        if !locked {
            consume_topic(topic, &consumer_id, Some(my_did)).await;
        }
    }

    // 1. Metadata-safe per-pair v2 DM queues (already bootstrapped at
    //    invite/handshake time; re-join here is a no-op if still subscribed).
    let v2_topics = dms::my_v2_topics().await;
    for (topic, _consumer, boot) in &v2_topics {
        ensure_joined(topic, boot).await;
    }
    // Confirm an INBOUND topic neighbor BEFORE draining the queue. The old
    // fire-and-forget regraft (connect + gossip_join_peers) returned BEFORE
    // iroh-gossip's NeighborUp fired, so consume_v2_queue ran against an
    // empty plumtree active_view and silently saw nothing — the receive-side
    // twin of the send-side no-op bug (the neighbor-gate fix 6a9770b cured
    // sends but not receives). The runtime's own chat path
    // (chat_cmd::attach_room_peer_until_joined) graft-then-POLLS
    // list_topic_peers until a neighbor is confirmed before treating the
    // topic as usable — mirror it. has_topic_peer() keeps warm topics at
    // zero added latency; only a decayed/never-formed neighbor pays a wait.
    //
    // CHEAP COLD-TOPIC GATE: on NATIVE hey-core is fake-async on a
    // current_thread executor, so join_all-ing many per-topic
    // wait_for_topic_peers (each up to 20×150ms ≈ 3s) does NOT overlap them —
    // the awaits still run STRUCTURALLY SERIALLY (n×3s). So instead of paying a
    // per-topic 3s poll, we (1) KICK every cold topic's regraft quickly (each is
    // just a dial+graft round-trip, no 3s poll), then (2) do ONE short bounded
    // shared wait (~800ms total, NOT per-topic) for those grafts to settle into
    // NeighborUp. Stragglers that don't form a neighbor in time are NOT dropped:
    // EVERY subscribed topic is still drained unconditionally below, and a topic
    // that wasn't ready this cycle gets re-kicked + drained on the next 1s poll
    // (plus the 2s background self-heal re-forms decayed neighbors). The latency
    // win comes from this cheap gate + the faster poll/heal + the immediate
    // outbox flush at send time — NOT from join_all overlap (which doesn't
    // happen on native). The kicks are fired with join_all (harmless on native,
    // a real overlap on wasm).
    let mut cold: Vec<(String, Vec<String>)> = Vec::new();
    for (topic, _consumer, boot) in &v2_topics {
        if !boot.is_empty() && !peer::has_topic_peer(topic).await {
            cold.push((topic.clone(), boot.clone()));
        }
    }
    if !cold.is_empty() {
        // (1) Fire a quick (re)graft at every cold topic and return — no per-
        //     topic poll loop (regraft_topic does one dial+graft, no 3s wait).
        let kicks = cold
            .iter()
            .map(|(topic, boot)| peer::regraft_topic(topic, boot));
        let _ = futures_util::future::join_all(kicks).await;
        // (2) ONE short bounded SHARED wait for the grafts to reach NeighborUp,
        //     re-checking has_topic_peer so a warm/just-formed topic exits early.
        //     ~800ms total (8×100ms), NOT per-topic, so the worst case is a fixed
        //     small floor regardless of how many topics are cold.
        for _ in 0..8 {
            sleep_ms(100).await;
            let mut any_cold = false;
            for (topic, _) in &cold {
                if !peer::has_topic_peer(topic).await {
                    any_cold = true;
                }
            }
            if !any_cold {
                break;
            }
        }
    }
    // DRAIN COVERAGE: drain EVERY subscribed v2 topic each poll (warm topics
    // that were never in `cold`, just-grafted ones, AND stragglers that didn't
    // form a neighbor this cycle — those simply have nothing to read and get
    // another shot next poll). Do not skip any topic.
    for (topic, consumer, _boot) in &v2_topics {
        if !locked {
            consume_v2_queue(topic, consumer).await;
        }
    }

    // 1b. F-LEGACY-PAIR-TOPIC (re-fix): actually LEAVE the leaky legacy
    //     deterministic pair topic once its SELF-owned grace has lapsed.
    //     my_v2_topics only STOPS RETURNING it after the grace, but this loop
    //     (and ensure_joined) only ever ADDS topics — so without an explicit
    //     leave the gossip provider stays subscribed to the DID-derivable
    //     SHA256(DID‖DID) topic forever (the metadata leak survives the timeout).
    //     Peer-independent + idempotent (runs the leave once per topic). Drains
    //     above still ran this cycle, so nothing in-grace is dropped early.
    dms::reconcile_legacy_topics().await;

    // 2. Retry any sends that failed transiently (needs the DEK to seal — skip
    //    while locked; the outbox persists and flushes on unlock).
    if !locked {
        outbox::flush().await;
    }

    Ok(())
}

/// Pull pending entries from a v2 per-pair queue. Entries are
/// `{ type: "dm.v2", envelope }` (NOT SignedEvents) — hand each wire string
/// to dms::receive_v2_wire which decrypts + verifies the inner sig.
async fn consume_v2_queue(topic: &str, consumer_id: &str) {
    let args = peer::RecvArgs {
        topic,
        limit: RECV_LIMIT,
        consumer_id,
        // v2 sender_ids are random per-contact pseudonyms; the inner sig
        // path drops our own loopback if it ever happens.
        skip_sender_id: None,
    };
    let resp = match peer::recv(args).await {
        Ok(v) => v,
        Err(_) => return,
    };
    // The provider wraps payloads in {status:"ok", data:{messages:[...]}} and
    // the gateway proxy passes that through unchanged, so `messages` lives
    // under `data`. Fall back to top-level for a flat provider response.
    let Some(arr) = resp
        .get("data")
        .and_then(|d| d.get("messages"))
        .or_else(|| resp.get("messages"))
        .and_then(|m| m.as_array())
        .cloned()
    else {
        return;
    };
    for entry in arr {
        // peer v1.1 body field is `content`; older builds used `message`.
        let Some(wire) = entry
            .get("content")
            .or_else(|| entry.get("message"))
            .and_then(|m| m.as_str())
        else {
            continue;
        };
        // Reassemble fragmented wires (the PQ handshake is ~23 KB, over the
        // ~4 KB gossip cap, so it arrives as ordered fragments). A non-fragment
        // wire passes straight through; an incomplete fragment set yields None.
        let Some(full) = crate::api::frag::reassemble(wire) else {
            continue;
        };
        if let Err(e) = dms::receive_v2_wire(topic, &full).await {
            crate::plat::warn(&format!("[hey-core] v2 dm consume: {e}"));
        }
    }
}

/// Pull SignedEvent entries from a plain topic (the legacy DM inbox).
async fn consume_topic(topic: &str, consumer_id: &str, my_did: Option<&str>) {
    let args = peer::RecvArgs {
        topic,
        limit: RECV_LIMIT,
        consumer_id,
        skip_sender_id: my_did,
    };
    let resp = match peer::recv(args).await {
        Ok(v) => v,
        Err(_) => return,
    };
    // Same provider envelope as consume_v2_queue: messages live under `data`.
    let Some(arr) = resp
        .get("data")
        .and_then(|d| d.get("messages"))
        .or_else(|| resp.get("messages"))
        .and_then(|m| m.as_array())
        .cloned()
    else {
        return;
    };
    for entry in arr {
        let Some(wire) = entry
            .get("content")
            .or_else(|| entry.get("message"))
            .and_then(|m| m.as_str())
        else {
            continue;
        };
        let Some(evt) = from_wire_string(wire) else {
            continue;
        };
        if verify_signed_event(&evt) != VerifyResult::Valid {
            continue;
        }
        if let Err(e) = route(&evt.event_type, &evt.payload, &evt.sender_did).await {
            crate::plat::warn(&format!("[hey-core] route {}: {e}", evt.event_type));
        }
    }
}

/// Route a verified SignedEvent to a registered app handler. DMs travel the v2
/// sealed-sender queue path (not SignedEvents), so the engine no longer special-
/// cases `dm.message` here. Event types dispatch purely to an app handler, or
/// are ignored if none is registered (hey-chat registers none).
async fn route(event_type: &str, payload: &Value, sender_did: &str) -> Result<(), String> {
    let handler = HANDLERS.with(|m| m.borrow().get(event_type).cloned());
    if let Some(h) = handler {
        return h(
            event_type.to_string(),
            payload.clone(),
            sender_did.to_string(),
        )
        .await;
    }
    Ok(())
}

async fn sleep_ms(ms: i32) {
    crate::plat::sleep_ms(ms).await;
}
