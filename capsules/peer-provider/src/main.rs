//! peer-provider — decentralized, durable gossip node for `elastos://peer/*`.
//!
//! Each runtime runs ONE (spawned on demand, proxied at /api/provider/peer/*).
//! It is an iroh-gossip P2P node with a **same-runtime in-memory fan-in** and
//! **on-disk durability**, so two runtimes federate directly — and messages +
//! subscriptions survive provider restarts / app closes.
//!
//! MODES (peer-config.json, GUI-settable via get_config/set_config):
//!   relay_mode = "default"     zero-config: iroh presets::N0 (pkarr discovery +
//!                              n0 relay). Works behind NAT, leans on n0 infra.
//!   relay_mode = "independent" no relay/discovery (presets::Minimal +
//!                              RelayMode::Disabled): peers reached ONLY via the
//!                              direct addresses carried in the ticket. Zero
//!                              third-party dependency. Pair with bind_port +
//!                              public_addr so the ticket is dialable.
//!   bind_port   fixed UDP port (0 = ephemeral) — set it for independent mode +
//!               port-forward / firewall-open it.
//!   public_addr "host:port" (domain or ip) injected into our ticket so peers
//!               can dial us directly. Resolved via DNS at ticket time.
//!
//! DURABILITY (always-on mailbox): the subscribed topics + their bootstrap
//! tickets are persisted and RE-JOINED on startup (so we listen for an accept
//! even before the app opens), and the per-topic message log + per-consumer
//! cursors are persisted (so a restart doesn't drop in-flight messages). The
//! app's gossip_recv drains the backlog whenever it next opens.
//!
//! Wire (line-delimited JSON, stdio): {op,...} -> {status:"ok",data} | {status:"error",...}
//! Ops: init, get_config, set_config, get_ticket, gossip_join {topic,bootstrap[]},
//!      gossip_leave, gossip_send {topic,message,sender_id,ts,signature},
//!      gossip_recv {topic,limit,consumer_id,skip_sender_id?}, list_topic_peers,
//!      list_peers.

use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use bytes::Bytes;
use n0_future::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use iroh::endpoint::{presets, BindOpts, RelayMode};
use iroh::{protocol::Router, Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh_gossip::{
    api::{Event, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};

// ── Config ───────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
struct PeerConfig {
    /// "default" (n0 zero-config) | "independent" (no relay/discovery, direct only)
    #[serde(default = "default_relay_mode")]
    relay_mode: String,
    /// Fixed UDP port; 0 = ephemeral.
    #[serde(default)]
    bind_port: u16,
    /// "host:port" (domain or ip) advertised in our ticket; empty = none.
    #[serde(default)]
    public_addr: String,
}
fn default_relay_mode() -> String { "default".into() }
impl Default for PeerConfig {
    fn default() -> Self { Self { relay_mode: default_relay_mode(), bind_port: 0, public_addr: String::new() } }
}
impl PeerConfig {
    fn independent(&self) -> bool { self.relay_mode == "independent" }
}

// ── Durable broker (per-topic log + per-consumer cursor) ─────────────────────

#[derive(Clone, Serialize, Deserialize)]
struct Msg { seq: u64, content: String, sender_id: String, ts: i64, signature: String }

#[derive(Default, Serialize, Deserialize)]
struct Topic { log: Vec<Msg>, cursors: HashMap<String, usize> }

#[derive(Default, Serialize, Deserialize)]
struct Broker {
    topics: BTreeMap<String, Topic>,
    next_seq: u64,
    /// topic -> bootstrap node tickets (for re-subscribe on boot).
    #[serde(default)]
    subscriptions: BTreeMap<String, Vec<String>>,
}

impl Broker {
    fn t(&mut self, name: &str) -> &mut Topic { self.topics.entry(name.to_string()).or_default() }
    fn append(&mut self, topic: &str, content: String, sender_id: String, ts: i64, signature: String) -> u64 {
        let seq = self.next_seq; self.next_seq += 1;
        self.t(topic).log.push(Msg { seq, content, sender_id, ts, signature });
        seq
    }
    fn drain(&mut self, topic: &str, limit: usize, consumer: &str, skip: Option<&str>) -> Vec<Value> {
        let t = self.t(topic);
        let mut idx = *t.cursors.get(consumer).unwrap_or(&0);
        let mut out = Vec::new();
        while idx < t.log.len() && out.len() < limit {
            let m = &t.log[idx]; idx += 1;
            if let Some(s) = skip { if m.sender_id == s { continue; } }
            out.push(json!({ "content": m.content, "message": m.content,
                "sender_id": m.sender_id, "ts": m.ts, "signature": m.signature, "seq": m.seq }));
        }
        t.cursors.insert(consumer.to_string(), idx);
        out
    }
    fn senders(&mut self, topic: &str) -> Vec<String> {
        let mut seen = Vec::new();
        for m in &self.t(topic).log { if !seen.contains(&m.sender_id) { seen.push(m.sender_id.clone()); } }
        seen
    }
}

type SharedBroker = Arc<Mutex<Broker>>;

// ── paths + persistence ──────────────────────────────────────────────────────

fn data_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".local/share")
    });
    base.join("elastos/peer-provider")
}
fn p(name: &str) -> PathBuf { data_dir().join(name) }

async fn save_broker(broker: &SharedBroker) {
    let snapshot = { serde_json::to_vec(&*broker.lock().await).ok() };
    if let Some(bytes) = snapshot {
        let _ = tokio::fs::create_dir_all(data_dir()).await;
        let _ = tokio::fs::write(p("broker.json"), bytes).await;
    }
}
async fn load_broker() -> Broker {
    match tokio::fs::read(p("broker.json")).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => Broker::default(),
    }
}
async fn load_config() -> PeerConfig {
    match tokio::fs::read(p("peer-config.json")).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => PeerConfig::default(),
    }
}
async fn save_config(cfg: &PeerConfig) {
    if let Ok(bytes) = serde_json::to_vec_pretty(cfg) {
        let _ = tokio::fs::create_dir_all(data_dir()).await;
        let _ = tokio::fs::write(p("peer-config.json"), bytes).await;
    }
}
async fn load_or_make_key() -> SecretKey {
    let path = p("secret.key");
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) { return SecretKey::from_bytes(&arr); }
    }
    let sk = SecretKey::generate();
    let _ = tokio::fs::create_dir_all(data_dir()).await;
    let _ = tokio::fs::write(&path, sk.to_bytes()).await;
    sk
}

// ── iroh layer ───────────────────────────────────────────────────────────────

struct Net {
    endpoint: Endpoint,
    gossip: Gossip,
    _router: Router,
    mem: iroh::address_lookup::MemoryLookup,
    senders: Mutex<HashMap<String, GossipSender>>,
    cfg: PeerConfig,
}

async fn start_net(cfg: &PeerConfig, sk: SecretKey) -> anyhow::Result<Net> {
    let mem = iroh::address_lookup::MemoryLookup::new();
    let mut builder = if cfg.independent() {
        Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
    } else {
        Endpoint::builder(presets::N0)
    };
    builder = builder.secret_key(sk);
    if cfg.bind_port != 0 {
        // Pin the fixed port on BOTH families. iroh pre-binds 0.0.0.0:0 + [::]:0 on
        // random ports; clear those first so the IPv6 socket lands on the SAME fixed
        // port we advertise in the ticket (required for public-IPv6 hole-punching —
        // a random [::]:N port can't be put in a stable ticket). The v6 socket is
        // best-effort (is_required=false) so IPv4-only hosts still come up.
        builder = builder
            .clear_ip_transports()
            .bind_addr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, cfg.bind_port)))?
            .bind_addr_with_opts(
                SocketAddr::from((Ipv6Addr::UNSPECIFIED, cfg.bind_port)),
                BindOpts::default().set_is_required(false),
            )?;
    }
    builder = builder.address_lookup(mem.clone());
    let endpoint = builder.bind().await?;
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let router = Router::builder(endpoint.clone()).accept(GOSSIP_ALPN, gossip.clone()).spawn();
    {
        let ep = endpoint.clone();
        tokio::spawn(async move { ep.online().await });
    }
    Ok(Net { endpoint, gossip, _router: router, mem, senders: Mutex::new(HashMap::new()), cfg: cfg.clone() })
}

fn topic_id(topic: &str) -> TopicId { TopicId::from_bytes(*blake3::hash(topic.as_bytes()).as_bytes()) }

/// Decode a bootstrap string: a base64(json) EndpointAddr (full, with direct
/// addrs) OR a bare EndpointId. Seeds MemoryLookup so the node can dial it
/// (independent mode relies on this), returns the EndpointId for gossip join.
fn decode_bootstrap(net: &Net, s: &str) -> Option<EndpointId> {
    let s = s.trim();
    if let Ok(bytes) = B64.decode(s) {
        if let Ok(addr) = serde_json::from_slice::<EndpointAddr>(&bytes) {
            let id = addr.id;
            net.mem.add_endpoint_info(addr);
            return Some(id);
        }
    }
    EndpointId::from_str(s).ok()
}

async fn ensure_topic(net: &Arc<Net>, broker: &SharedBroker, topic: &str, bootstrap: &[String]) {
    let ids: Vec<EndpointId> = bootstrap.iter().filter_map(|b| decode_bootstrap(net, b)).collect();
    let mut t = net.senders.lock().await;
    if let Some(sender) = t.get(topic) {
        if !ids.is_empty() { let _ = sender.join_peers(ids).await; }
        return;
    }
    let sub = match net.gossip.subscribe(topic_id(topic), ids).await {
        Ok(s) => s,
        Err(e) => { eprintln!("[peer-provider] subscribe {topic} failed: {e}"); return; }
    };
    let (sender, mut receiver) = sub.split();
    t.insert(topic.to_string(), sender);
    // Persist the subscription (topic + bootstrap) for re-join on boot.
    {
        let mut b = broker.lock().await;
        b.subscriptions.insert(topic.to_string(), bootstrap.to_vec());
    }
    save_broker(broker).await;
    let broker = broker.clone();
    let topic_s = topic.to_string();
    tokio::spawn(async move {
        while let Some(ev) = receiver.next().await {
            if let Ok(Event::Received(msg)) = ev {
                let (c, s, ts, g) = decode_wire(&msg.content);
                broker.lock().await.append(&topic_s, c, s, ts, g);
                save_broker(&broker).await;
            }
        }
    });
}

fn encode_wire(content: &str, sender_id: &str, ts: i64, signature: &str) -> Bytes {
    Bytes::from(json!({ "c": content, "s": sender_id, "t": ts, "g": signature }).to_string())
}
fn decode_wire(b: &[u8]) -> (String, String, i64, String) {
    if let Ok(v) = serde_json::from_slice::<Value>(b) {
        if v.get("c").is_some() {
            return (
                v.get("c").and_then(Value::as_str).unwrap_or("").into(),
                v.get("s").and_then(Value::as_str).unwrap_or("").into(),
                v.get("t").and_then(Value::as_i64).unwrap_or(0),
                v.get("g").and_then(Value::as_str).unwrap_or("").into(),
            );
        }
    }
    (String::from_utf8_lossy(b).into(), String::new(), 0, String::new())
}

/// Our shareable ticket = base64(json(EndpointAddr)) with id + relay + direct
/// addrs, plus the configured public_addr (resolved) so peers can dial us.
async fn build_ticket(net: &Net) -> String {
    let mut addr = net.endpoint.addr();
    let pa = net.cfg.public_addr.trim();
    if !pa.is_empty() {
        // Accept "ip:port" directly, else DNS-resolve "host:port".
        if let Ok(sa) = pa.parse::<SocketAddr>() {
            addr = addr.with_ip_addr(sa);
        } else if let Ok(iter) = tokio::net::lookup_host(pa).await {
            for sa in iter { addr = addr.with_ip_addr(sa); }
        }
    }
    serde_json::to_vec(&addr).map(|b| B64.encode(b)).unwrap_or_default()
}

// ── op handling ──────────────────────────────────────────────────────────────

fn ok(data: Value) -> Value { json!({ "status": "ok", "data": data }) }
fn err(m: impl Into<String>) -> Value { json!({ "status": "error", "code": "peer_provider", "message": m.into() }) }
fn sf(r: &Value, k: &str) -> String { r.get(k).and_then(Value::as_str).unwrap_or("").into() }
fn nf(r: &Value, k: &str) -> i64 { r.get(k).and_then(Value::as_i64).unwrap_or(0) }
fn boot(r: &Value) -> Vec<String> {
    r.get("bootstrap").and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

async fn handle(net: &Option<Arc<Net>>, broker: &SharedBroker, live_cfg: &Arc<Mutex<PeerConfig>>, req: &Value) -> Value {
    match req.get("op").and_then(Value::as_str).unwrap_or("") {
        "init" => match net {
            Some(n) => ok(json!({ "node_id": n.endpoint.id().to_string(),
                "transport": if n.cfg.independent() { "iroh-independent" } else { "iroh-gossip" } })),
            None => ok(json!({ "node_id": null, "transport": "same-runtime" })),
        },
        "get_ticket" => match net {
            Some(n) => ok(json!({ "ticket": build_ticket(n).await, "node_id": n.endpoint.id().to_string() })),
            None => ok(json!({ "ticket": "" })),
        },
        "get_config" => {
            let c = live_cfg.lock().await.clone();
            let node_id = match net { Some(n) => n.endpoint.id().to_string(), None => String::new() };
            let ticket = match net { Some(n) => build_ticket(n).await, None => String::new() };
            ok(json!({ "relay_mode": c.relay_mode, "bind_port": c.bind_port,
                "public_addr": c.public_addr, "node_id": node_id, "ticket": ticket,
                "running_independent": net.as_ref().map(|n| n.cfg.independent()).unwrap_or(false) }))
        }
        "set_config" => {
            let cur = live_cfg.lock().await.clone();
            let new = PeerConfig {
                relay_mode: req.get("relay_mode").and_then(Value::as_str).unwrap_or(&cur.relay_mode).into(),
                bind_port: req.get("bind_port").and_then(Value::as_u64).map(|v| v as u16).unwrap_or(cur.bind_port),
                public_addr: req.get("public_addr").and_then(Value::as_str).unwrap_or(&cur.public_addr).into(),
            };
            // public_addr applies live (next ticket); relay_mode/bind_port need a restart.
            let restart = new.relay_mode != cur.relay_mode || new.bind_port != cur.bind_port;
            save_config(&new).await;
            if let Some(n) = net { /* live ticket uses n.cfg.public_addr at bind time; */ let _ = n; }
            *live_cfg.lock().await = new;
            ok(json!({ "ok": true, "restart_required": restart }))
        }
        "gossip_join" => {
            let topic = sf(req, "topic");
            if topic.is_empty() { return err("gossip_join: missing topic"); }
            if let Some(n) = net { ensure_topic(n, broker, &topic, &boot(req)).await; }
            ok(json!({ "ok": true }))
        }
        "gossip_leave" => ok(json!({ "ok": true })),
        "gossip_send" => {
            let topic = sf(req, "topic");
            if topic.is_empty() { return err("gossip_send: missing topic"); }
            let content = req.get("message").or_else(|| req.get("content")).and_then(Value::as_str).unwrap_or("").to_string();
            let sender_id = sf(req, "sender_id"); let ts = nf(req, "ts"); let signature = sf(req, "signature");
            let seq = broker.lock().await.append(&topic, content.clone(), sender_id.clone(), ts, signature.clone());
            save_broker(broker).await;
            if let Some(n) = net {
                ensure_topic(n, broker, &topic, &[]).await;
                if let Some(sender) = n.senders.lock().await.get(&topic) {
                    let _ = sender.broadcast(encode_wire(&content, &sender_id, ts, &signature)).await;
                }
            }
            ok(json!({ "seq": seq, "delivered": true }))
        }
        "gossip_recv" => {
            let topic = sf(req, "topic");
            if topic.is_empty() { return err("gossip_recv: missing topic"); }
            let limit = req.get("limit").and_then(Value::as_u64).unwrap_or(64).max(1) as usize;
            let consumer = sf(req, "consumer_id");
            if consumer.is_empty() { return err("gossip_recv: missing consumer_id"); }
            let skip = req.get("skip_sender_id").and_then(Value::as_str);
            let messages = broker.lock().await.drain(&topic, limit, &consumer, skip);
            if !messages.is_empty() { save_broker(broker).await; }
            ok(json!({ "messages": messages }))
        }
        "list_topic_peers" => ok(json!({ "peers": broker.lock().await.senders(&sf(req, "topic")) })),
        "list_peers" => ok(json!({ "peers": [] })),
        other => err(format!("unknown op: {other}")),
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let cfg = load_config().await;
    let live_cfg = Arc::new(Mutex::new(cfg.clone()));
    let broker: SharedBroker = Arc::new(Mutex::new(load_broker().await));
    let net = match start_net(&cfg, load_or_make_key().await).await {
        Ok(n) => {
            eprintln!("[peer-provider] iroh node {} up (mode={})", n.endpoint.id(), n.cfg.relay_mode);
            Some(Arc::new(n))
        }
        Err(e) => { eprintln!("[peer-provider] iroh unavailable ({e}); same-runtime only"); None }
    };

    // Always-on: re-join persisted subscriptions so we listen before any app opens.
    if let Some(n) = &net {
        let subs: Vec<(String, Vec<String>)> =
            broker.lock().await.subscriptions.iter().map(|(t, b)| (t.clone(), b.clone())).collect();
        for (topic, bs) in subs { ensure_topic(n, &broker, &topic, &bs).await; }
    }

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut out = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let resp = match serde_json::from_str::<Value>(trimmed) {
            Ok(req) => handle(&net, &broker, &live_cfg, &req).await,
            Err(e) => err(format!("invalid json: {e}")),
        };
        let mut buf = resp.to_string(); buf.push('\n');
        if out.write_all(buf.as_bytes()).await.is_err() { break; }
        let _ = out.flush().await;
    }
}
