//! peer-provider — gossip broker for the `elastos://peer/*` scheme.
//!
//! THREE MODES, one binary:
//!
//!   (default)         stdio JSON-RPC, LOCAL in-memory broker. Delivers between
//!                     capsules on the SAME runtime (two browser sessions on one
//!                     server). No network. This is what the runtime spawns when
//!                     PEER_HUB_URL is unset.
//!
//!   PEER_HUB_URL set  stdio JSON-RPC, but every gossip op is FORWARDED over
//!                     HTTPS to a shared hub. Capsules on DIFFERENT runtimes that
//!                     point at the same hub deliver to each other — i.e. your
//!                     YunoHost server and a friend's YunoHost server federate.
//!                     `init` is answered locally so the provider still registers
//!                     if the hub is momentarily down; ops then fail soft and the
//!                     capsule's outbox retries.
//!
//!   --hub             Run the broker as an HTTP service (THE hub). One public box
//!                     runs this; every runtime's provider sets PEER_HUB_URL to it.
//!                     Port: $HEY_PEER_HUB_PORT (default 8765); put TLS in front
//!                     (nginx). Optional shared secret: $HEY_PEER_HUB_TOKEN.
//!
//! The hub is CONTENT-BLIND: Hey DMs are E2E sealed-sender (ciphertext envelopes
//! + random per-contact pseudonyms — see hey-core dms.rs), so the hub only ever
//! sees opaque blobs on topic names. The identical gossip_* op contract across
//! all modes means the transport can later graduate to a Boson/Carrier V2 DHT
//! with no change to hey-core, the apps, or the runtime.
//!
//! Wire protocol (stdio AND hub HTTP body) mirrors blobs-provider/ipfs-provider:
//!   request:  { "op": "...", ... }
//!   response: { "status": "ok", "data": <value> }
//!           | { "status": "error", "code": "peer_provider", "message": "..." }
//!
//! Ops: init, gossip_join, gossip_leave, gossip_send, gossip_recv,
//!      list_topic_peers, list_peers, get_ticket. See handle().

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::time::Duration;

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

/// The single source of truth for op semantics — used by the local broker
/// (standalone mode) and by the hub (HTTP mode). Client mode does NOT call
/// this; it forwards to the hub, which calls it there.
fn handle(broker: &mut Broker, req: &Value) -> Value {
    let op = req.get("op").and_then(Value::as_str).unwrap_or("");
    match op {
        "init" => ok(json!({ "node_id": "peer-broker", "transport": "broker" })),

        "gossip_join" | "gossip_leave" => {
            let _ = s(req, "topic");
            ok(json!({ "ok": true }))
        }

        "gossip_send" => {
            let topic = s(req, "topic");
            if topic.is_empty() {
                return err("gossip_send: missing topic");
            }
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

fn read_line_loop(mut on_request: impl FnMut(&Value) -> Value) {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(trimmed) {
            Ok(req) => on_request(&req),
            Err(e) => err(format!("invalid json: {e}")),
        };
        if writeln!(out, "{resp}").is_err() {
            break;
        }
        let _ = out.flush();
    }
}

/// Default mode: stdio JSON-RPC backed by a local in-memory broker.
fn run_standalone() {
    let mut broker = Broker::default();
    read_line_loop(|req| handle(&mut broker, req));
}

/// Federated mode: stdio JSON-RPC, but every op except `init` is forwarded to
/// the shared hub over HTTPS so capsules on other runtimes (pointed at the same
/// hub) receive it. `init` is local so the provider registers even if the hub
/// is briefly unreachable.
fn run_client(hub_url: String) {
    let token = std::env::var("HEY_PEER_HUB_TOKEN").ok().filter(|t| !t.is_empty());
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build();
    eprintln!("[peer-provider] federated mode -> hub {hub_url}");
    read_line_loop(|req| {
        let op = req.get("op").and_then(Value::as_str).unwrap_or("");
        if op == "init" {
            return ok(json!({ "node_id": "peer-hub-client", "transport": "hub", "hub": hub_url }));
        }
        let body = req.to_string();
        let mut http = agent.post(&hub_url).set("content-type", "application/json");
        if let Some(ref t) = token {
            http = http.set("x-hey-peer-token", t);
        }
        match http.send_string(&body) {
            Ok(resp) => match resp.into_string() {
                Ok(txt) => serde_json::from_str::<Value>(txt.trim())
                    .unwrap_or_else(|e| err(format!("hub returned bad json: {e}"))),
                Err(e) => err(format!("hub read failed: {e}")),
            },
            Err(e) => err(format!("hub unreachable: {e}")),
        }
    });
}

/// Hub mode: run the broker as an HTTP service. One public box runs this; every
/// runtime's provider points PEER_HUB_URL at it. Single-threaded — gossip volume
/// is low and shared Broker state stays lock-free.
fn run_hub() -> ! {
    let port: u16 = std::env::var("HEY_PEER_HUB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765);
    let token = std::env::var("HEY_PEER_HUB_TOKEN").ok().filter(|t| !t.is_empty());
    let server = tiny_http::Server::http(("0.0.0.0", port))
        .unwrap_or_else(|e| panic!("peer-provider hub: cannot bind 0.0.0.0:{port}: {e}"));
    eprintln!(
        "[peer-provider] hub listening on 0.0.0.0:{port} (auth: {})",
        if token.is_some() { "token" } else { "none" }
    );
    let mut broker = Broker::default();
    for mut request in server.incoming_requests() {
        if let Some(ref t) = token {
            let authed = request
                .headers()
                .iter()
                .any(|h| h.field.equiv("X-Hey-Peer-Token") && h.value.as_str() == t);
            if !authed {
                let _ = request.respond(
                    tiny_http::Response::from_string(err("unauthorized").to_string())
                        .with_status_code(401),
                );
                continue;
            }
        }
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        let resp = match serde_json::from_str::<Value>(body.trim()) {
            Ok(req) => handle(&mut broker, &req),
            Err(e) => err(format!("invalid json: {e}")),
        };
        let header =
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
        let _ = request.respond(tiny_http::Response::from_string(resp.to_string()).with_header(header));
    }
    std::process::exit(0);
}

fn main() {
    if std::env::args().any(|a| a == "--hub") {
        run_hub();
    }
    match std::env::var("PEER_HUB_URL") {
        Ok(url) if !url.trim().is_empty() => run_client(url.trim().to_string()),
        _ => run_standalone(),
    }
}
