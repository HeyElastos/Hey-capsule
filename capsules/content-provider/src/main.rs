//! content-provider — `elastos://content/*` adapter on top of kubo.
//!
//! Hey-social used to call `ipfs.add_bytes` directly. The runtime
//! contract says app capsules should NOT touch ipfs-provider (system-
//! only); they go through `content/*` instead. The content provider:
//!   * wraps kubo's HTTP API for raw byte storage
//!   * maps capsule-level policy hints ("network_default", "local_pin",
//!     "transient") to pin lifecycle
//!   * returns a small availability receipt so the capsule can prove
//!     "this CID exists" without re-fetching
//!
//! Future direction (out of v0): a separate dDRM / encryption gate
//! before publish; supernode replication for `network_default` policy;
//! TTL eviction for `transient`. The wire shape stays the same.
//!
//! Wire protocol: line-delimited JSON on stdin/stdout. Mirrors
//! blobs-provider's ProviderResponse envelope.
//!
//! Operations (matching hey-social's runtime.rs::content callers):
//!
//!   init                                       → { protocol_version,
//!                                                  provider, features }
//!   publish   { data (b64), filename, policy } → { payload: { cid,
//!                                                             policy,
//!                                                             ts,
//!                                                             filename },
//!                                                  signer_did: "",
//!                                                  signature: "" }
//!   fetch     { cid, path? }                   → { data (b64), size }
//!   ensure    { cid, policy }                  → { cid, policy }
//!   unpublish { cid }                          → { cid }
//!   shutdown                                   → ok
//!
//! Configuration: env vars
//!   CONTENT_PROVIDER_KUBO_API   (default http://127.0.0.1:5001)

use std::time::SystemTime;

use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const DEFAULT_KUBO_API: &str = "http://127.0.0.1:5001";

// ── Wire ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Init {},
    Publish {
        data: String,
        #[serde(default)]
        filename: String,
        #[serde(default = "default_policy")]
        policy: String,
    },
    Fetch {
        cid: String,
        #[serde(default)]
        path: Option<String>,
    },
    Ensure {
        cid: String,
        #[serde(default = "default_policy")]
        policy: String,
    },
    Unpublish {
        cid: String,
    },
    Shutdown {},
}

fn default_policy() -> String {
    "network_default".into()
}

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
    fn err_code(code: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: msg.into(),
        }
    }
}

// ── Node ─────────────────────────────────────────────────────────────

struct Node {
    client: reqwest::Client,
    kubo_api: String,
}

#[derive(Debug, Deserialize)]
struct AddResponse {
    #[serde(rename = "Hash")]
    hash: String,
    #[serde(default, rename = "Size")]
    size: String,
}

impl Node {
    fn new() -> Self {
        let kubo_api = std::env::var("CONTENT_PROVIDER_KUBO_API")
            .unwrap_or_else(|_| DEFAULT_KUBO_API.into());
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
            kubo_api,
        }
    }

    async fn publish_bytes(
        &self,
        bytes: Vec<u8>,
        filename: &str,
        policy: &str,
    ) -> Result<serde_json::Value> {
        // kubo /api/v0/add accepts multipart form; the part name is
        // the original filename, the body is the raw bytes.
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let url = format!("{}/api/v0/add?cid-version=1&raw-leaves=true", self.kubo_api);
        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .context("kubo /add request")?;
        if !resp.status().is_success() {
            anyhow::bail!("kubo /add returned {}", resp.status());
        }
        // /add streams one JSON object per file. We sent one file →
        // one object.
        let text = resp.text().await?;
        let line = text
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty /add response"))?;
        let added: AddResponse = serde_json::from_str(line)
            .with_context(|| format!("parse /add: {line}"))?;

        // Apply policy. local_pin + network_default → pin. transient
        // → leave unpinned so kubo's GC eventually reaps it.
        match policy {
            "transient" => {}
            _ => {
                self.pin(&added.hash).await?;
            }
        }
        let ts = now_secs();
        Ok(serde_json::json!({
            "payload": {
                "cid": added.hash,
                "size": added.size,
                "filename": filename,
                "policy": policy,
                "ts": ts,
            },
            // Reserved for the future "signed availability receipt"
            // shape. Empty for v0 — recipients should accept missing.
            "signer_did": "",
            "signature": "",
        }))
    }

    async fn fetch(&self, cid: &str, path: Option<&str>) -> Result<serde_json::Value> {
        let target = match path {
            Some(p) => format!("{}/{}", cid, p.trim_start_matches('/')),
            None => cid.to_string(),
        };
        let url = format!("{}/api/v0/cat?arg={}", self.kubo_api, urlencode(&target));
        // Fail-fast + retry. A single long `cat` HANGS on a COLD bitswap session
        // (e.g. right after a runtime restart, before the kubo<->peer link warms)
        // — wedging the caller for the whole client timeout and starving every other
        // request behind the provider's stdio. Bound each attempt and retry a few
        // times so a transient cold fetch recovers on a later attempt (bitswap
        // warms between tries) instead of blocking, and a genuinely-unreachable
        // CID returns an ERROR in bounded time so the caller can poll again.
        let mut last_err = anyhow::anyhow!("kubo /cat: no attempt made");
        for attempt in 1..=4u32 {
            match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                self.cat_once(&url),
            )
            .await
            {
                Ok(Ok(bytes)) => {
                    return Ok(serde_json::json!({
                        "data": B64.encode(&bytes),
                        "size": bytes.len(),
                        "cid": cid,
                    }));
                }
                Ok(Err(e)) => last_err = e,
                Err(_) => {
                    last_err = anyhow::anyhow!("kubo /cat timed out (15s, attempt {attempt}/4)")
                }
            }
            if attempt < 4 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
        Err(last_err)
    }

    async fn cat_once(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.client.post(url).send().await.context("kubo /cat")?;
        if !resp.status().is_success() {
            anyhow::bail!("kubo /cat returned {}", resp.status());
        }
        Ok(resp.bytes().await?.to_vec())
    }

    async fn pin(&self, cid: &str) -> Result<()> {
        let url = format!("{}/api/v0/pin/add?arg={}", self.kubo_api, urlencode(cid));
        let resp = self.client.post(&url).send().await.context("kubo /pin/add")?;
        if !resp.status().is_success() {
            anyhow::bail!("kubo /pin/add returned {}", resp.status());
        }
        Ok(())
    }

    async fn unpin(&self, cid: &str) -> Result<()> {
        let url = format!("{}/api/v0/pin/rm?arg={}", self.kubo_api, urlencode(cid));
        let resp = self.client.post(&url).send().await.context("kubo /pin/rm")?;
        // /pin/rm returns 500 when the CID isn't pinned; treat as
        // idempotent success.
        let s = resp.status();
        if !s.is_success() && s.as_u16() != 500 {
            anyhow::bail!("kubo /pin/rm returned {}", s);
        }
        Ok(())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn urlencode(s: &str) -> String {
    // Minimal URL-encoding for CIDs / IPFS paths (alnum + ./_- pass
    // through; everything else becomes %XX). Kubo accepts the
    // unencoded form too in practice but we hedge.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/') {
            out.push(c);
        } else {
            for b in c.to_string().bytes() {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

// ── Dispatch ─────────────────────────────────────────────────────────

async fn handle(node: &tokio::sync::Mutex<Option<Node>>, req: Request) -> Response {
    match req {
        Request::Init {} => {
            let mut guard = node.lock().await;
            if guard.is_none() {
                *guard = Some(Node::new());
            }
            Response::ok(serde_json::json!({
                "protocol_version": "0.1",
                "provider": "content",
                "features": ["publish", "fetch", "ensure", "unpublish"],
            }))
        }
        Request::Publish {
            data,
            filename,
            policy,
        } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err_code("not_init", "init first");
            };
            let bytes = match B64.decode(&data) {
                Ok(b) => b,
                Err(e) => {
                    return Response::err_code("bad_data", format!("base64: {e}"));
                }
            };
            match n.publish_bytes(bytes, &filename, &policy).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err_code("publish_failed", format!("{e:#}")),
            }
        }
        Request::Fetch { cid, path } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err_code("not_init", "init first");
            };
            match n.fetch(&cid, path.as_deref()).await {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err_code("fetch_failed", format!("{e:#}")),
            }
        }
        Request::Ensure { cid, policy } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err_code("not_init", "init first");
            };
            // Ensure = re-assert the policy. "transient" unpins;
            // anything else pins.
            let res = if policy == "transient" {
                n.unpin(&cid).await
            } else {
                n.pin(&cid).await
            };
            match res {
                Ok(_) => Response::ok(serde_json::json!({ "cid": cid, "policy": policy })),
                Err(e) => Response::err_code("ensure_failed", format!("{e:#}")),
            }
        }
        Request::Unpublish { cid } => {
            let guard = node.lock().await;
            let Some(n) = guard.as_ref() else {
                return Response::err_code("not_init", "init first");
            };
            match n.unpin(&cid).await {
                Ok(_) => Response::ok(serde_json::json!({ "cid": cid })),
                Err(e) => Response::err_code("unpublish_failed", format!("{e:#}")),
            }
        }
        Request::Shutdown {} => {
            let mut guard = node.lock().await;
            *guard = None;
            Response::ok(serde_json::json!({ "message": "Provider shutting down" }))
        }
    }
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

    let node: tokio::sync::Mutex<Option<Node>> = tokio::sync::Mutex::new(None);
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
            Err(e) => Response::err_code("invalid_request", format!("{e}")),
        };
        let mut out = serde_json::to_vec(&resp)?;
        out.push(b'\n');
        stdout.write_all(&out).await?;
        stdout.flush().await?;
    }
    Ok(())
}
