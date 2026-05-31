//! peer-provider — same-runtime gossip broker for the `elastos://peer/*` scheme.
//!
//! Wire protocol mirrors `blobs-provider` / `ipfs-provider`: line-delimited JSON
//! requests on stdin, line-delimited JSON responses on stdout. The runtime
//! spawns ONE instance per host and proxies every capsule's
//! `/api/provider/peer/<op>` call to it, so all capsules on this runtime share
//! the same broker state — which is exactly what lets two browser sessions
//! (e.g. a DM inviter and an invitee) exchange messages with no relay.
//!
//! Operations (the contract hey-core's `runtime::peer` calls):
//!   init                                                       -> { node_id }
//!   gossip_join   { topic }                                    -> { ok: true }
//!   gossip_leave  { topic }                                    -> { ok: true }
//!   gossip_send   { topic, message, sender_id, ts, signature } -> { seq }
//!   gossip_recv   { topic, limit, consumer_id, skip_sender_id? }
//!                                       -> { messages: [{ content, sender_id, ts, signature, seq }] }
//!   list_topic_peers { topic }                                 -> { peers: [sender_id] }
//!   list_peers    {}                                           -> { peers: [] }
//!   get_ticket    {}                                           -> { ticket: "" }
//!
//! Delivery model: each topic keeps an append-only log; each `consumer_id`
//! keeps a cursor into that log. `gossip_recv` returns the slice the consumer
//! has not seen yet (skipping `skip_sender_id`, e.g. the caller's own DID), then
//! advances the cursor. New consumers start at 0 so a queue joined BEFORE the
//! first send (the invite case) never misses the opening message.
//!
//! Scope: this tier is SAME-RUNTIME only. Cross-runtime delivery is a future
//! swap of the log's backing store for a Boson (Carrier V2) DHT node behind the
//! identical op contract — see the note in the pack docs. State is in-memory:
//! messages queued but unconsumed are lost if the runtime restarts the process.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

#[derive(Clone)]
struct Msg {
    seq: u64,
    content: String,
    sender_id: String,
    ts: i64,
    signature: String,
}

#[derive(Default)]
struct Topic {
    log: Vec<Msg>,
    /// consumer_id -> next unread index into `log`.
    cursors: HashMap<String, usize>,
}

#[derive(Default)]
struct Broker {
    topics: HashMap<String, Topic>,
    next_seq: u64,
}

impl Broker {
    fn topic(&mut self, name: &str) -> &mut Topic {
        self.topics.entry(name.to_string()).or_default()
    }

    fn send(&mut self, topic: &str, content: String, sender_id: String, ts: i64, signature: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.topic(topic).log.push(Msg { seq, content, sender_id, ts, signature });
        seq
    }

    fn recv(&mut self, topic: &str, limit: usize, consumer_id: &str, skip: Option<&str>) -> Vec<Value> {
        let t = self.topic(topic);
        let mut idx = *t.cursors.get(consumer_id).unwrap_or(&0);
        let mut out = Vec::new();
        while idx < t.log.len() && out.len() < limit {
            let m = &t.log[idx];
            idx += 1; // advance past EVERY examined entry, incl. skipped ones
            if let Some(s) = skip {
                if m.sender_id == s {
                    continue;
                }
            }
            out.push(json!({
                "content": m.content,
                "message": m.content, // legacy field name some builds read
                "sender_id": m.sender_id,
                "ts": m.ts,
                "signature": m.signature,
                "seq": m.seq,
            }));
        }
        t.cursors.insert(consumer_id.to_string(), idx);
        out
    }

    fn topic_senders(&mut self, topic: &str) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for m in &self.topic(topic).log {
            if !seen.contains(&m.sender_id) {
                seen.push(m.sender_id.clone());
            }
        }
        seen
    }
}

/// Response envelope matching elastos-runtime's ProviderResponse:
///   { "status": "ok", "data": <value> } | { "status": "error", "code", "message" }
fn ok(data: Value) -> Value {
    json!({ "status": "ok", "data": data })
}
fn err(msg: impl Into<String>) -> Value {
    json!({ "status": "error", "code": "peer_provider", "message": msg.into() })
}

fn s(req: &Value, key: &str) -> String {
    req.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}
fn i(req: &Value, key: &str) -> i64 {
    req.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn handle(broker: &mut Broker, req: &Value) -> Value {
    let op = req.get("op").and_then(Value::as_str).unwrap_or("");
    match op {
        "init" => ok(json!({ "node_id": "peer-broker", "transport": "same-runtime" })),

        "gossip_join" | "gossip_leave" => {
            // Joining/leaving is implicit in the broker — recv with a
            // consumer_id is what actually drives delivery. Ack so the
            // capsule's join-before-publish handshake proceeds.
            let _ = s(req, "topic");
            ok(json!({ "ok": true }))
        }

        "gossip_send" => {
            let topic = s(req, "topic");
            if topic.is_empty() {
                return err("gossip_send: missing topic");
            }
            // hey-core sends the body under `message`; accept `content` too.
            let content = req
                .get("message")
                .or_else(|| req.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let seq = broker.send(&topic, content, s(req, "sender_id"), i(req, "ts"), s(req, "signature"));
            ok(json!({ "seq": seq, "delivered": true }))
        }

        "gossip_recv" => {
            let topic = s(req, "topic");
            if topic.is_empty() {
                return err("gossip_recv: missing topic");
            }
            let limit = req.get("limit").and_then(Value::as_u64).unwrap_or(64).max(1) as usize;
            let consumer_id = s(req, "consumer_id");
            if consumer_id.is_empty() {
                return err("gossip_recv: missing consumer_id");
            }
            let skip = req.get("skip_sender_id").and_then(Value::as_str);
            let messages = broker.recv(&topic, limit, &consumer_id, skip);
            ok(json!({ "messages": messages }))
        }

        "list_topic_peers" => {
            let topic = s(req, "topic");
            ok(json!({ "peers": broker.topic_senders(&topic) }))
        }
        "list_peers" => ok(json!({ "peers": [] })),
        "get_ticket" => ok(json!({ "ticket": "" })),

        other => err(format!("unknown op: {other}")),
    }
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut broker = Broker::default();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin closed
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(trimmed) {
            Ok(req) => handle(&mut broker, &req),
            Err(e) => err(format!("invalid json: {e}")),
        };
        // One JSON object per line; flush so the runtime bridge sees it promptly.
        if writeln!(out, "{resp}").is_err() {
            break;
        }
        let _ = out.flush();
    }
}
