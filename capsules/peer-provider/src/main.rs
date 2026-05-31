//! peer-provider — decentralized gossip node for the `elastos://peer/*` scheme.
//!
//! Each runtime runs ONE of these (the runtime spawns it on demand and proxies
//! every capsule's /api/provider/peer/<op> to it). It is an **iroh-gossip P2P
//! node**: two runtimes whose capsules subscribe to the same topic exchange
//! messages directly, peer-to-peer, with NO central hub and NO config — node
//! discovery + NAT traversal ride iroh's pkarr/relay defaults (presets::N0).
//!
//! Zero-config federation works because the capsule carries the *other* node's
//! id through Hey's own channels: the DM invite link and the IPFS-published
//! profile both include `get_ticket` output (this node's EndpointId), and the
//! joining side passes it as `bootstrap` to `gossip_join`. Discovery resolves
//! the id to an address; the gossip mesh forms; messages flow both ways.
//!
//! A small in-memory per-topic log + per-consumer cursor sits in front of the
//! mesh: `gossip_send` appends locally AND broadcasts to the mesh; inbound mesh
//! messages are appended too; `gossip_recv` just drains the log by cursor. That
//! one buffer unifies SAME-runtime delivery (two browser sessions on one node)
//! and CROSS-runtime delivery (the mesh) behind the identical op contract. If
//! iroh fails to start (e.g. no network at boot), the node degrades to
//! same-runtime-only — the provider still registers and local chat still works.
//!
//! Wire protocol (line-delimited JSON on stdio) mirrors blobs-provider:
//!   request:  { "op": "...", ... }
//!   response: { "status": "ok", "data": <value> } | { "status":"error", ... }
//!
//! Ops: init, gossip_join {topic, bootstrap?[]}, gossip_leave {topic},
//!      gossip_send {topic, message, sender_id, ts, signature},
//!      gossip_recv {topic, limit, consumer_id, skip_sender_id?},
//!      list_topic_peers, list_peers, get_ticket -> { ticket: <EndpointId> }.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use bytes::Bytes;
use n0_future::StreamExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use iroh::{endpoint::presets, protocol::Router, Endpoint, EndpointId, SecretKey};
use iroh_gossip::{
    api::{Event, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};

// ── In-memory delivery buffer (per topic log + per consumer cursor) ──────────

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
    cursors: HashMap<String, usize>,
}

#[derive(Default)]
struct Broker {
    topics: HashMap<String, Topic>,
    next_seq: u64,
}

impl Broker {
    fn t(&mut self, name: &str) -> &mut Topic {
        self.topics.entry(name.to_string()).or_default()
    }
    fn append(&mut self, topic: &str, content: String, sender_id: String, ts: i64, signature: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.t(topic).log.push(Msg { seq, content, sender_id, ts, signature });
        seq
    }
    fn drain(&mut self, topic: &str, limit: usize, consumer: &str, skip: Option<&str>) -> Vec<Value> {
        let t = self.t(topic);
        let mut idx = *t.cursors.get(consumer).unwrap_or(&0);
        let mut out = Vec::new();
        while idx < t.log.len() && out.len() < limit {
            let m = &t.log[idx];
            idx += 1;
            if let Some(s) = skip {
                if m.sender_id == s {
                    continue;
                }
            }
            out.push(json!({
                "content": m.content, "message": m.content,
                "sender_id": m.sender_id, "ts": m.ts,
                "signature": m.signature, "seq": m.seq,
            }));
        }
        t.cursors.insert(consumer.to_string(), idx);
        out
    }
    fn senders(&mut self, topic: &str) -> Vec<String> {
        let mut seen = Vec::new();
        for m in &self.t(topic).log {
            if !seen.contains(&m.sender_id) {
                seen.push(m.sender_id.clone());
            }
        }
        seen
    }
}

// ── iroh mesh layer ──────────────────────────────────────────────────────────

struct Net {
    endpoint: Endpoint,
    gossip: Gossip,
    _router: Router,
    /// topic string -> live gossip sender (present once joined).
    topics: Mutex<HashMap<String, GossipSender>>,
}

type SharedBroker = Arc<Mutex<Broker>>;

fn data_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".local/share")
    });
    base.join("elastos/peer-provider")
}

/// Load a persisted node key (stable EndpointId across restarts so tickets stay
/// valid) or generate + save one.
async fn load_or_make_key(dir: &PathBuf) -> SecretKey {
    let path = dir.join("secret.key");
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return SecretKey::from_bytes(&arr);
        }
    }
    let sk = SecretKey::generate();
    let _ = tokio::fs::create_dir_all(dir).await;
    let _ = tokio::fs::write(&path, sk.to_bytes()).await;
    sk
}

async fn start_net() -> anyhow::Result<Net> {
    let dir = data_dir();
    let sk = load_or_make_key(&dir).await;
    let endpoint = Endpoint::builder(presets::N0).secret_key(sk).bind().await?;
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let router = Router::builder(endpoint.clone())
        .accept(GOSSIP_ALPN, gossip.clone())
        .spawn();
    // Publish our address to discovery in the background so peers can find us by id.
    {
        let ep = endpoint.clone();
        tokio::spawn(async move { ep.online().await });
    }
    Ok(Net { endpoint, gossip, _router: router, topics: Mutex::new(HashMap::new()) })
}

fn topic_id(topic: &str) -> TopicId {
    TopicId::from_bytes(*blake3::hash(topic.as_bytes()).as_bytes())
}

/// Ensure we're subscribed to `topic` (joining with any `bootstrap` peers), and
/// that a background task drains inbound mesh messages into the shared broker.
async fn ensure_topic(net: &Arc<Net>, broker: &SharedBroker, topic: &str, bootstrap: Vec<EndpointId>) {
    {
        let mut t = net.topics.lock().await;
        if let Some(sender) = t.get(topic) {
            if !bootstrap.is_empty() {
                let _ = sender.join_peers(bootstrap).await;
            }
            return;
        }
        // Subscribe (non-blocking — the inviter has no bootstrap yet).
        let sub = match net.gossip.subscribe(topic_id(topic), bootstrap).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[peer-provider] subscribe {topic} failed: {e}");
                return;
            }
        };
        let (sender, mut receiver) = sub.split();
        t.insert(topic.to_string(), sender);
        // Drain inbound mesh messages into the broker log.
        let broker = broker.clone();
        let topic_s = topic.to_string();
        tokio::spawn(async move {
            while let Some(ev) = receiver.next().await {
                if let Ok(Event::Received(msg)) = ev {
                    let (content, sender_id, ts, signature) = decode_wire(&msg.content);
                    broker.lock().await.append(&topic_s, content, sender_id, ts, signature);
                }
            }
        });
    }
}

/// Mesh payload = the broker fields as compact JSON bytes.
fn encode_wire(content: &str, sender_id: &str, ts: i64, signature: &str) -> Bytes {
    Bytes::from(json!({ "c": content, "s": sender_id, "t": ts, "g": signature }).to_string())
}
fn decode_wire(b: &[u8]) -> (String, String, i64, String) {
    if let Ok(v) = serde_json::from_slice::<Value>(b) {
        if v.get("c").is_some() {
            return (
                v.get("c").and_then(Value::as_str).unwrap_or("").to_string(),
                v.get("s").and_then(Value::as_str).unwrap_or("").to_string(),
                v.get("t").and_then(Value::as_i64).unwrap_or(0),
                v.get("g").and_then(Value::as_str).unwrap_or("").to_string(),
            );
        }
    }
    // Fallback: treat raw bytes as the content.
    (String::from_utf8_lossy(b).to_string(), String::new(), 0, String::new())
}

// ── op handling ──────────────────────────────────────────────────────────────

fn ok(data: Value) -> Value { json!({ "status": "ok", "data": data }) }
fn err(msg: impl Into<String>) -> Value {
    json!({ "status": "error", "code": "peer_provider", "message": msg.into() })
}
fn sf(req: &Value, k: &str) -> String { req.get(k).and_then(Value::as_str).unwrap_or("").to_string() }
fn nf(req: &Value, k: &str) -> i64 { req.get(k).and_then(Value::as_i64).unwrap_or(0) }

fn bootstrap_ids(req: &Value) -> Vec<EndpointId> {
    req.get("bootstrap")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter_map(|s| EndpointId::from_str(s.trim()).ok()).collect())
        .unwrap_or_default()
}

async fn handle(net: &Option<Arc<Net>>, broker: &SharedBroker, req: &Value) -> Value {
    let op = req.get("op").and_then(Value::as_str).unwrap_or("");
    match op {
        "init" => {
            let node = match net {
                Some(n) => json!({ "node_id": n.endpoint.id().to_string(), "transport": "iroh-gossip" }),
                None => json!({ "node_id": null, "transport": "same-runtime" }),
            };
            ok(node)
        }
        "get_ticket" => match net {
            Some(n) => ok(json!({ "ticket": n.endpoint.id().to_string() })),
            None => ok(json!({ "ticket": "" })),
        },
        "gossip_join" => {
            let topic = sf(req, "topic");
            if topic.is_empty() {
                return err("gossip_join: missing topic");
            }
            if let Some(n) = net {
                ensure_topic(n, broker, &topic, bootstrap_ids(req)).await;
            }
            ok(json!({ "ok": true }))
        }
        "gossip_leave" => ok(json!({ "ok": true })),
        "gossip_send" => {
            let topic = sf(req, "topic");
            if topic.is_empty() {
                return err("gossip_send: missing topic");
            }
            let content = req.get("message").or_else(|| req.get("content"))
                .and_then(Value::as_str).unwrap_or("").to_string();
            let sender_id = sf(req, "sender_id");
            let ts = nf(req, "ts");
            let signature = sf(req, "signature");
            // Local append (same-runtime consumers) ...
            let seq = broker.lock().await.append(&topic, content.clone(), sender_id.clone(), ts, signature.clone());
            // ... and broadcast to the mesh (cross-runtime).
            if let Some(n) = net {
                ensure_topic(n, broker, &topic, vec![]).await;
                if let Some(sender) = n.topics.lock().await.get(&topic) {
                    let _ = sender.broadcast(encode_wire(&content, &sender_id, ts, &signature)).await;
                }
            }
            ok(json!({ "seq": seq, "delivered": true }))
        }
        "gossip_recv" => {
            let topic = sf(req, "topic");
            if topic.is_empty() {
                return err("gossip_recv: missing topic");
            }
            let limit = req.get("limit").and_then(Value::as_u64).unwrap_or(64).max(1) as usize;
            let consumer = sf(req, "consumer_id");
            if consumer.is_empty() {
                return err("gossip_recv: missing consumer_id");
            }
            let skip = req.get("skip_sender_id").and_then(Value::as_str);
            let messages = broker.lock().await.drain(&topic, limit, &consumer, skip);
            ok(json!({ "messages": messages }))
        }
        "list_topic_peers" => ok(json!({ "peers": broker.lock().await.senders(&sf(req, "topic")) })),
        "list_peers" => ok(json!({ "peers": [] })),
        other => err(format!("unknown op: {other}")),
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let broker: SharedBroker = Arc::new(Mutex::new(Broker::default()));
    let net = match start_net().await {
        Ok(n) => {
            eprintln!("[peer-provider] iroh node {} up", n.endpoint.id());
            Some(Arc::new(n))
        }
        Err(e) => {
            eprintln!("[peer-provider] iroh unavailable ({e}); same-runtime only");
            None
        }
    };

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut out = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(trimmed) {
            Ok(req) => handle(&net, &broker, &req).await,
            Err(e) => err(format!("invalid json: {e}")),
        };
        let mut buf = resp.to_string();
        buf.push('\n');
        if out.write_all(buf.as_bytes()).await.is_err() {
            break;
        }
        let _ = out.flush().await;
    }
}
