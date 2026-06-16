//! blobs-provider — iroh-blobs direct peer-to-peer file transfer.
//!
//! Wire protocol mirrors `ipfs-provider`: line-delimited JSON requests on
//! stdin, line-delimited JSON responses on stdout. Persistent FsStore under
//! $XDG_DATA_HOME/elastos/blobs-provider so blobs survive restarts.
//!
//! Operations:
//!   init                                   start endpoint + store + router
//!   add_path  { path }                     -> { hash, ticket }
//!   add_bytes { data_base64 }              -> { hash, ticket }    (small files only)
//!   fetch     { ticket, dest }             -> { hash, bytes }
//!   share     { hash }                     -> { ticket }
//!   list                                   -> { blobs: [{ hash }] }
//!   drop      { hash }                     -> { ok }
//!
//! Phase 1: scaffold of the real iroh-blobs API. Some operations are stubbed
//! pending verification against a running node — the goal of this phase is
//! the end-to-end send/recv test in `src/bin/transfer_test.rs`.
//!
//! Federation pinning (store-and-forward): the endpoint key is persisted so the
//! EndpointId is stable across restarts. When `blobs-federation.json` (or the
//! `HEY_BLOB_FEDERATION_*` env) configures peers/pin, the provider joins a fixed
//! gossip topic on the SAME endpoint, ANNOUNCES `{hash, ticket}` on every add, and
//! (if `pin`) pulls a copy of every announced blob into its own store. `fetch`
//! then falls back across those peer holders, so a blob survives the original
//! sender going offline. Zero hey-core / runtime changes — all of it is here.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use bytes::Bytes;
use iroh::address_lookup::MemoryLookup;
use iroh::{endpoint::presets, protocol::Router, Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh_blobs::{store::fs::FsStore, ticket::BlobTicket, BlobsProtocol, Hash, HashAndFormat};
use iroh_gossip::{
    api::{Event, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use n0_future::StreamExt;
use serde::{Deserialize, Serialize};
use std::future::IntoFuture;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[allow(dead_code)]
enum Request {
    Init {},
    AddPath { path: String },
    AddBytes { data_base64: String },
    Fetch { ticket: String, dest: String },
    Share { hash: String },
    List {},
    Drop { hash: String },
}

// Wire protocol matches elastos-runtime's ProviderResponse (bridge.rs):
//   { "status": "ok",    "data": <value> }
//   { "status": "error", "code": "<short>", "message": "<long>" }
// Anything else and the Init handshake fails with BridgeError::InitFailed,
// which the runtime logs at debug! and the provider never registers.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Response {
    Ok {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },
    Error {
        code: String,
        message: String,
    },
}

impl Response {
    fn ok(data: serde_json::Value) -> Self {
        Self::Ok { data: Some(data) }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self::Error { code: "blobs_provider".into(), message: msg.into() }
    }
}

struct Node {
    endpoint: Endpoint,
    store: FsStore,
    /// Federation peers (always-on holders) tried as fallback in `fetch` and used
    /// to bootstrap the pin gossip mesh. Empty on a plain user box = today's behavior.
    fed_peers: Vec<EndpointId>,
    /// Broadcast handle for the federation topic; `None` when federation is off
    /// (no peers configured and not a pin node), so `announce` is a no-op.
    fed_sender: Mutex<Option<GossipSender>>,
    _router: Router,
}

impl Node {
    async fn spawn(data_dir: PathBuf, fed: FedConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&data_dir).await.ok();
        // STABLE identity: persist the endpoint secret so our EndpointId survives
        // restarts. Without this the id is random each launch and a peer's
        // configured node-id (and any minted ticket) goes stale on every reboot.
        let sk = load_or_make_key(&data_dir).await;
        let mem = MemoryLookup::new();
        let fed_peers: Vec<EndpointId> =
            fed.peers.iter().filter_map(|s| decode_peer(&mem, s)).collect();
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(sk)
            .address_lookup(mem)
            .bind()
            .await?;
        let store = FsStore::load(&data_dir).await?;
        let blobs = BlobsProtocol::new(&store, None);
        let gossip = Gossip::builder().spawn(endpoint.clone());
        // One endpoint serves BOTH the blobs ALPN and the gossip ALPN — the Router
        // dispatches each inbound connection by its negotiated ALPN.
        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs)
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();
        {
            let ep = endpoint.clone();
            tokio::spawn(async move {
                ep.online().await;
            });
        }
        // Federation pin mesh: only spin it up when there is something to do —
        // peers to reach or a pin duty. A bare user box (no config) skips gossip
        // entirely and behaves exactly as before.
        let fed_sender = Mutex::new(None);
        if !fed_peers.is_empty() || fed.pin {
            match gossip.subscribe(fed_topic_id(), fed_peers.clone()).await {
                Ok(topic) => {
                    let (sender, mut receiver) = topic.split();
                    *fed_sender.lock().await = Some(sender);
                    // Always drain the receiver (avoids gossip backpressure); only
                    // pin nodes act on announcements.
                    let do_pin = fed.pin;
                    let store2 = store.clone();
                    let ep2 = endpoint.clone();
                    tokio::spawn(async move {
                        while let Some(ev) = receiver.next().await {
                            if let Ok(Event::Received(msg)) = ev {
                                if do_pin {
                                    pin_announced(&store2, &ep2, &msg.content).await;
                                }
                            }
                        }
                    });
                    eprintln!(
                        "[blobs-provider] federation up: {} peer(s), pin={}",
                        fed_peers.len(),
                        fed.pin
                    );
                }
                Err(e) => eprintln!("[blobs-provider] federation subscribe failed: {e}"),
            }
        }
        Ok(Self { endpoint, store, fed_peers, fed_sender, _router: router })
    }

    async fn add_path(&self, path: PathBuf) -> Result<(String, String)> {
        let abs = std::path::absolute(&path).context("absolute path")?;
        let tag = self.store.blobs().add_path(abs).await?;
        let ticket = BlobTicket::new(self.endpoint.id().into(), tag.hash, tag.format);
        let (hash_s, ticket_s) = (tag.hash.to_string(), ticket.to_string());
        // Tell the federation a new blob exists so pin nodes pull a copy while we
        // (the holder) are still online — that copy is what survives us going away.
        self.announce(&hash_s, &ticket_s).await;
        Ok((hash_s, ticket_s))
    }

    /// Broadcast `{hash, ticket}` on the federation topic. No-op when federation
    /// is off (`fed_sender` is None).
    async fn announce(&self, hash: &str, ticket: &str) {
        let guard = self.fed_sender.lock().await;
        if let Some(sender) = guard.as_ref() {
            let msg = Bytes::from(serde_json::json!({ "hash": hash, "ticket": ticket }).to_string());
            if let Err(e) = sender.broadcast(msg).await {
                eprintln!("[blobs-provider] federation announce failed: {e}");
            }
        }
    }

    /// Download the blob for `ticket` directly P2P from a holder and return its
    /// BYTES inline. A WASM capsule shares no filesystem with this host process,
    /// so it cannot read an exported file — the bytes must ride back in the
    /// response. The caller (hey-core) chunks to <=256 KiB, so the base64 stays
    /// under the provider IPC body limit. Export to a provider-side temp, read
    /// it, return it, delete it.
    async fn fetch(&self, ticket_str: &str) -> Result<(String, Vec<u8>)> {
        let ticket: BlobTicket = ticket_str.parse()?;
        let hash = ticket.hash();
        let downloader = self.store.downloader(&self.endpoint);
        // Try the ticket's own holder first, then each federation peer. Content is
        // BLAKE3 hash-addressed, so ANY holder serves the same hash — this is what
        // lets a blob survive the original sender going offline once a federation
        // box holds a copy. Each attempt is bounded so a dead holder can't hang the
        // whole fetch; we move to the next target on error OR timeout.
        let mut targets: Vec<EndpointId> = vec![ticket.addr().id];
        for peer in &self.fed_peers {
            if !targets.contains(peer) {
                targets.push(*peer);
            }
        }
        let mut last_err: Option<anyhow::Error> = None;
        for node in targets {
            match timeout(
                Duration::from_secs(20),
                downloader.download(hash, Some(node)).into_future(),
            )
            .await
            {
                Ok(Ok(())) => {
                    last_err = None;
                    break;
                }
                Ok(Err(e)) => last_err = Some(anyhow::anyhow!("download from {node}: {e}")),
                Err(_) => last_err = Some(anyhow::anyhow!("download from {node} timed out")),
            }
        }
        // Every target failed — but only error out if we genuinely don't hold the
        // blob. A concurrent federation pin (or a prior partial that completed) may
        // have landed it locally, in which case we can still serve it.
        if let Some(e) = last_err {
            if !self.store.blobs().has(hash).await.unwrap_or(false) {
                return Err(e);
            }
        }
        let abs = std::path::absolute(tempfile_path()?).context("absolute tmp")?;
        self.store.blobs().export(hash, abs.clone()).await?;
        let bytes = tokio::fs::read(&abs).await.context("read exported blob")?;
        let _ = tokio::fs::remove_file(&abs).await;
        Ok((hash.to_string(), bytes))
    }
}

// ── federation pinning ────────────────────────────────────────────────────────
//
// The always-on boxes hold copies of blobs (store-and-forward) so a large file is
// still fetchable after the original sender goes offline. All of it lives in this
// provider — no runtime patch, no hey-core Attachment change. A sender's provider
// ANNOUNCES `{hash, ticket}` on a fixed gossip topic when it adds a blob; pin
// boxes self-download that hash (pulled while the sender is still online) into
// their own FsStore. FsStore runs NO GC by default, so a held blob persists; we
// also tag it so it survives even if GC is ever enabled. The fetch fallback above
// then serves it from the pin box when the original holder is gone.

#[derive(Clone, Serialize, Deserialize, Default)]
struct FedConfig {
    /// Peer holders: a bare EndpointId, or base64(json(EndpointAddr)) with addrs.
    #[serde(default)]
    peers: Vec<String>,
    /// True on always-on boxes: pull+hold a copy of every announced blob.
    #[serde(default)]
    pin: bool,
}

/// Read federation config from `blobs-federation.json` in the data dir, with env
/// override/seed (`HEY_BLOB_FEDERATION_PEERS` comma-separated, `HEY_BLOB_FEDERATION_PIN`).
/// The file is the durable source of truth (survives the runtime relaunching the
/// provider); env is a convenience for boxes that can inject it.
async fn load_fed_config(data_dir: &PathBuf) -> FedConfig {
    let mut cfg: FedConfig = match tokio::fs::read(data_dir.join("blobs-federation.json")).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => FedConfig::default(),
    };
    if let Ok(env) = std::env::var("HEY_BLOB_FEDERATION_PEERS") {
        let peers: Vec<String> = env
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !peers.is_empty() {
            cfg.peers = peers;
        }
    }
    if let Ok(p) = std::env::var("HEY_BLOB_FEDERATION_PIN") {
        let v = p.trim();
        if v == "1" || v.eq_ignore_ascii_case("true") {
            cfg.pin = true;
        }
    }
    cfg
}

/// Decode a federation peer string into an EndpointId. Accepts base64(json(
/// EndpointAddr)) (seeds the MemoryLookup with its dialable addrs, for relay-
/// independent reach) or a bare EndpointId (dialable via n0 DNS under presets::N0).
fn decode_peer(mem: &MemoryLookup, s: &str) -> Option<EndpointId> {
    let s = s.trim();
    if let Ok(bytes) = BASE64.decode(s) {
        if let Ok(addr) = serde_json::from_slice::<EndpointAddr>(&bytes) {
            let id = addr.id;
            mem.add_endpoint_info(addr);
            return Some(id);
        }
    }
    EndpointId::from_str(s).ok()
}

const FED_TOPIC: &str = "hey/blobs/federation/v1";
fn fed_topic_id() -> TopicId {
    TopicId::from_bytes(*blake3::hash(FED_TOPIC.as_bytes()).as_bytes())
}

/// Persist/load the endpoint secret so the EndpointId is stable across restarts.
async fn load_or_make_key(data_dir: &PathBuf) -> SecretKey {
    let path = data_dir.join("secret.key");
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return SecretKey::from_bytes(&arr);
        }
    }
    let sk = SecretKey::generate();
    let _ = tokio::fs::create_dir_all(data_dir).await;
    let _ = tokio::fs::write(&path, sk.to_bytes()).await;
    sk
}

/// Pin node: react to a `{hash, ticket}` announcement by pulling the blob from the
/// announcer into our own store (if we don't already hold it), then tagging it so
/// it is never GC-eligible. Bounded so a slow/absent announcer can't wedge us.
async fn pin_announced(store: &FsStore, endpoint: &Endpoint, content: &[u8]) {
    let v: serde_json::Value = match serde_json::from_slice(content) {
        Ok(v) => v,
        Err(_) => return,
    };
    let Some(ticket_str) = v.get("ticket").and_then(|t| t.as_str()) else {
        return;
    };
    let Ok(ticket) = ticket_str.parse::<BlobTicket>() else {
        return;
    };
    let hash: Hash = ticket.hash();
    if store.blobs().has(hash).await.unwrap_or(false) {
        return; // already mirrored
    }
    let downloader = store.downloader(endpoint);
    match timeout(
        Duration::from_secs(30),
        downloader.download(hash, Some(ticket.addr().id)).into_future(),
    )
    .await
    {
        Ok(Ok(())) => {
            // GC is off by default so the blob already persists; tag it too so a
            // pin survives even if a GC sweep is ever turned on.
            let _ = store
                .tags()
                .set(format!("pin/{hash}"), HashAndFormat::raw(hash))
                .await;
            eprintln!("[blobs-provider] federation pinned {hash}");
        }
        Ok(Err(e)) => eprintln!("[blobs-provider] federation pin {hash} failed: {e}"),
        Err(_) => eprintln!("[blobs-provider] federation pin {hash} timed out"),
    }
}

fn data_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local/share")
        });
    base.join("elastos/blobs-provider")
}

async fn handle(node: &Mutex<Option<Node>>, req: Request) -> Response {
    match req {
        Request::Init {} => {
            let mut guard = node.lock().await;
            if guard.is_some() {
                return Response::ok(serde_json::json!({ "already_initialized": true }));
            }
            let dir = data_dir();
            let fed = load_fed_config(&dir).await;
            match Node::spawn(dir, fed).await {
                Ok(n) => {
                    let node_id = n.endpoint.id().to_string();
                    *guard = Some(n);
                    Response::ok(serde_json::json!({ "node_id": node_id }))
                }
                Err(e) => Response::err(format!("init failed: {e:#}")),
            }
        }
        Request::AddPath { path } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err("not initialized — send `init` first");
            };
            match n.add_path(PathBuf::from(path)).await {
                Ok((hash, ticket)) => Response::ok(serde_json::json!({ "hash": hash, "ticket": ticket })),
                Err(e) => Response::err(format!("add_path failed: {e:#}")),
            }
        }
        Request::AddBytes { data_base64 } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err("not initialized");
            };
            let bytes = match BASE64.decode(&data_base64) {
                Ok(b) => b,
                Err(e) => return Response::err(format!("invalid base64: {e}")),
            };
            let tmp = match tempfile_path() {
                Ok(p) => p,
                Err(e) => return Response::err(format!("tempfile: {e:#}")),
            };
            if let Err(e) = tokio::fs::write(&tmp, &bytes).await {
                return Response::err(format!("write tmp: {e:#}"));
            }
            let result = n.add_path(tmp.clone()).await;
            let _ = tokio::fs::remove_file(&tmp).await;
            match result {
                Ok((hash, ticket)) => Response::ok(serde_json::json!({ "hash": hash, "ticket": ticket })),
                Err(e) => Response::err(format!("add_bytes failed: {e:#}")),
            }
        }
        Request::Fetch { ticket, dest: _ } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err("not initialized");
            };
            // Return the blob BYTES inline (base64) so a WASM capsule can receive
            // them — it has no filesystem shared with this host process. `dest`
            // is now vestigial; the provider uses its own temp sink.
            match n.fetch(&ticket).await {
                Ok((hash, bytes)) => {
                    Response::ok(serde_json::json!({ "hash": hash, "bytes": BASE64.encode(&bytes) }))
                }
                Err(e) => Response::err(format!("fetch failed: {e:#}")),
            }
        }
        Request::Share { hash } => {
            // For now, callers should retain the ticket from add_path. Re-minting a
            // ticket from a bare hash requires the BlobFormat, which we don't currently
            // persist alongside the hash. Phase 1 follow-up: keep a tag table.
            Response::err(format!(
                "share-by-hash not yet implemented (hash={hash}) — retain ticket from add_path"
            ))
        }
        Request::List {} => {
            // Phase 1 follow-up: iterate store.blobs() once method shape is verified.
            Response::err("list not yet implemented")
        }
        Request::Drop { hash: _ } => {
            // Phase 1 follow-up: tag drop + GC.
            Response::err("drop not yet implemented")
        }
    }
}

fn tempfile_path() -> Result<PathBuf> {
    let dir = std::env::temp_dir();
    let n: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos() as u64;
    Ok(dir.join(format!("blobs-provider-{n}.bin")))
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let node: Mutex<Option<Node>> = Mutex::new(None);
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => handle(&node, req).await,
            Err(e) => Response::err(format!("invalid request: {e}")),
        };
        let mut out = serde_json::to_vec(&resp)?;
        out.push(b'\n');
        stdout.write_all(&out).await?;
        stdout.flush().await?;
    }
    Ok(())
}
