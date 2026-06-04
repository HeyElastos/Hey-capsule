//! hey-social-cli — headless diagnostic for the Hey SOCIAL follow + feed flow.
//!
//! Mirrors hey-chat-cli, but for the SOCIAL plane: it drives the same hey-core
//! transport (peer gossip), identity, and content/IPFS the hey-social wasm app
//! runs, and speaks hey-social's interoperable federation events — so you can
//! prove, cross-runtime, that a NEW follower's feed materialises the followed
//! user's posts (not just the local user's).
//!
//! The mechanism it exercises end-to-end (the fix under test):
//!   * a publisher pins an index of its own posts to IPFS and announces the
//!     index head CID on `hey-v0/user/<did>/posts` as a `posts.head` event;
//!   * a follower joins that topic (+ its own follow inbox), and on `posts.head`
//!     fetches the index by CID and pulls every post it's missing → feed.
//! No IPNS/mutable pointer exists, so the head is discovered via the event and
//! the bytes are pulled by CID (which works cross-runtime — that's the point).
//!
//! Identity/auth are identical to hey-chat-cli: `--secret <attach_secret>`
//! mints a shell bearer on the loopback `/api/auth/attach`, and
//! `adopt_provider_identity()` adopts the runtime's "hey" did (wallet model).
//! Local storage is a fresh `--store` dir (default /tmp/hey-social-cli) — i.e.
//! the CLI is the real did with an EMPTY feed, exactly a "new follower".
//!
//! NOTE on post bodies: the real app encodes post bodies as dag-cbor (IPLD);
//! this CLI encodes a small JSON body. The follow.request / posts.head / index
//! envelope + topics are byte-interoperable with the app; only the post-body
//! codec differs, so CLI<->CLI proves the transport/discovery/fetch path fully,
//! and CLI<->app interoperates for follow + head + index discovery.

use hey_core::api::dms;
use hey_core::ctx::{init, CapsuleCtx};
use hey_core::events::{
    create_signed_event, from_wire_string, to_wire_string, verify_signed_event, VerifyResult,
};
use hey_core::runtime::peer::{self, PublishArgs, RecvArgs};
use hey_core::runtime::ipfs;
use hey_core::session;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;

// Per-capsule identity — byte-identical to capsules/hey-social/src/main.rs, so
// the CLI shares hey-social's storage namespace + "hey" signing identity.
const HEY_SOCIAL_CTX: CapsuleCtx = CapsuleCtx {
    capsule_id: "hey-social",
    private_namespace: "Hey",
    session_key: "hey-social-session",
    welcomed_key: "hey-social-welcomed",
    session_redeemed_key: "hey-session-redeemed",
    home_launch_token_key: "hey-home-launch-token",
    runtime_token_key: "hey-runtime-token",
    token_store_key: "hey-capability-tokens",
    route_mode_key: "hey-storage-route-mode",
    boot_capabilities: &[
        ("elastos://peer/*", "message"),
        ("elastos://content/*", "write"),
        ("elastos://did/*", "read"),
    ],
};

// ── Local storage keys (the CLI's own fresh feed/index, via plat kv) ──────
const KV_FEED: &str = "social-cli-feed";
const KV_OWN_INDEX: &str = "social-cli-own-index";
const KV_FOLLOWING: &str = "social-cli-following";

// FeedEntry shape is byte-compatible with hey-social's posts::FeedEntry, so the
// index this CLI publishes can be backfilled by the real app and vice-versa.
#[derive(Serialize, Deserialize, Clone)]
struct IdxEntry {
    id: String,
    ts: i64,
    author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    post_cid: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct FeedItem {
    id: String,
    ts: i64,
    author: String,
    post_cid: String,
    caption: String,
}

#[derive(Serialize, Deserialize)]
struct PostBody {
    id: String,
    author: String,
    caption: String,
    ts: i64,
}

// ── Minimal single-thread executor (every native leaf op blocks) ─────────
fn block_on<F: Future>(fut: F) -> F::Output {
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn raw() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            raw()
        }
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(clone, no_op, no_op, no_op))
    }
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

fn short(s: &str) -> String {
    if s.len() <= 14 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..8], &s[s.len() - 4..])
    }
}

fn now_ms() -> i64 {
    hey_core::plat::now_ms()
}

/// Read the `attach_secret` out of a runtime-coords.json (so the secret never
/// has to appear on a command line). The file is runtime-owned, so run the CLI
/// under sudo when pointing at the live coords.
fn secret_from_file(path: &str) -> Result<String, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("parse {path}: {e}"))?;
    v.get("attach_secret")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| format!("{path} has no attach_secret"))
}

// ── attach: mint a shell-scope bearer from the attach_secret ─────────────
fn attach(secret: &str) -> Result<String, String> {
    let url = format!("{}/api/auth/attach", hey_core::runtime::api_base());
    let body = json!({ "secret": secret, "scope": "shell" }).to_string();
    let (status, text) = hey_core::plat::http("POST", &url, Some(&body))?;
    if !(200..300).contains(&status) {
        return Err(format!("attach failed (HTTP {status}): {text}"));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("attach json: {e}"))?;
    v.get("token")
        .and_then(|t| t.as_str())
        .map(String::from)
        .ok_or_else(|| format!("attach response had no token: {text}"))
}

fn ensure_identity() -> String {
    if let Some(s) = session::current() {
        if s.did_key.starts_with("did:key:z") {
            return s.did_key;
        }
    }
    match block_on(dms::adopt_provider_identity()) {
        Some(did) => did,
        None => die("identity/whoami(ns=hey) returned no did — is the runtime signed in?"),
    }
}

// ── local kv helpers ─────────────────────────────────────────────────────
fn load<T: for<'de> Deserialize<'de> + Default>(key: &str) -> T {
    hey_core::plat::kv_get(key)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
fn save<T: Serialize>(key: &str, v: &T) {
    if let Ok(s) = serde_json::to_string(v) {
        hey_core::plat::kv_set(key, &s);
    }
}

// ── publish: pin my posts index + announce the head ──────────────────────
fn publish_and_announce_head(me_did: &str) -> Option<String> {
    let idx: Vec<IdxEntry> = load(KV_OWN_INDEX);
    let bytes = serde_json::to_vec(&idx).ok()?;
    let resp = block_on(ipfs::add_bytes(&bytes, "posts-index.json", true)).ok()?;
    let head = ipfs::extract_cid(&resp)?;
    let topic = format!("hey-v0/user/{me_did}/posts");
    publish_event(&topic, "posts.head", json!({ "head_cid": head }));
    Some(head)
}

fn publish_event(topic: &str, event_type: &str, payload: Value) {
    let evt = match block_on(create_signed_event(event_type, payload)) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("  ! sign {event_type}: {e}");
            return;
        }
    };
    let wire = to_wire_string(&evt);
    let r = block_on(peer::publish(PublishArgs {
        topic,
        message: &wire,
        sender_id: &evt.sender_did,
        ts: evt.ts,
        signature: &evt.signature,
    }));
    if let Err(e) = r {
        eprintln!("  ! publish {event_type} -> {topic}: {e}");
    }
}

// ── commands ──────────────────────────────────────────────────────────────
fn cmd_whoami() {
    let did = ensure_identity();
    let ticket = block_on(peer::my_ticket()).unwrap_or_default();
    println!("did:    {did}");
    println!("ticket: {ticket}");
}

fn cmd_post(caption: &str) {
    let me = ensure_identity();
    let id = format!("{:x}", now_ms());
    let ts = now_ms();
    let body = PostBody {
        id: id.clone(),
        author: me.clone(),
        caption: caption.to_string(),
        ts,
    };
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let resp = match block_on(ipfs::add_bytes(&bytes, "post.json", true)) {
        Ok(r) => r,
        Err(e) => die(&format!("ipfs add post: {e}")),
    };
    let cid = ipfs::extract_cid(&resp).unwrap_or_else(|| die("ipfs add returned no cid"));
    // local own index + feed
    let mut idx: Vec<IdxEntry> = load(KV_OWN_INDEX);
    idx.insert(
        0,
        IdxEntry { id: id.clone(), ts, author: me.clone(), post_cid: Some(cid.clone()) },
    );
    save(KV_OWN_INDEX, &idx);
    let mut feed: Vec<FeedItem> = load(KV_FEED);
    feed.insert(
        0,
        FeedItem { id, ts, author: me.clone(), post_cid: cid.clone(), caption: caption.to_string() },
    );
    save(KV_FEED, &feed);
    println!("posted: cid={} caption={caption:?}", short(&cid));
    match publish_and_announce_head(&me) {
        Some(h) => println!("announced posts.head (full): {h}\n  topic hey-v0/user/{me}/posts"),
        None => println!("(warning) could not publish/announce head"),
    }
}

fn cmd_follow(did: &str, ticket: &str) {
    let me = ensure_identity();
    if !did.starts_with("did:key:z") {
        die("follow needs a did:key:z…");
    }
    let boot: Vec<String> = if ticket.is_empty() { vec![] } else { vec![ticket.to_string()] };
    let _ = block_on(peer::join_topic_with(&format!("hey-v0/user/{did}/posts"), &boot));
    let _ = block_on(peer::join_topic_with(&format!("hey-v0/follow/{did}"), &boot));
    let mut following: Vec<String> = load(KV_FOLLOWING);
    if !following.contains(&did.to_string()) {
        following.push(did.to_string());
        save(KV_FOLLOWING, &following);
    }
    let my_ticket = block_on(peer::my_ticket()).unwrap_or_default();
    publish_event(
        &format!("hey-v0/follow/{did}"),
        "follow.request",
        json!({ "target_did": did, "from_name": "hey-social-cli", "from_ticket": my_ticket, "ts": now_ms() }),
    );
    println!("followed {} (joined posts+follow topics, sent follow.request)", short(did));
    println!("now run: hey-social-cli poll {did}   # to receive their posts.head + backfill");
    let _ = me;
}

fn backfill_from_index(author_did: &str, head_cid: &str) -> usize {
    let bytes = match block_on(ipfs::get_bytes(head_cid, None)) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("  ! fetch index {}: {e}", short(head_cid));
            return 0;
        }
    };
    let index: Vec<IdxEntry> = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  ! parse index: {e}");
            return 0;
        }
    };
    let mut feed: Vec<FeedItem> = load(KV_FEED);
    let have: std::collections::HashSet<String> = feed.iter().map(|f| f.post_cid.clone()).collect();
    let mut added = 0;
    for e in index {
        let Some(cid) = e.post_cid.clone() else { continue };
        if have.contains(&cid) {
            continue;
        }
        let Ok(pb) = block_on(ipfs::get_bytes(&cid, None)) else { continue };
        let Ok(body) = serde_json::from_slice::<PostBody>(&pb) else { continue };
        if body.author != author_did {
            continue;
        }
        feed.push(FeedItem {
            id: body.id,
            ts: body.ts,
            author: body.author,
            post_cid: cid.clone(),
            caption: body.caption,
        });
        added += 1;
        println!("  + backfilled post {} from {}", short(&cid), short(author_did));
    }
    if added > 0 {
        feed.sort_by(|a, b| b.ts.cmp(&a.ts));
        save(KV_FEED, &feed);
    }
    added
}

fn cmd_sync(author_did: &str, head_cid: &str) {
    let n = backfill_from_index(author_did, head_cid);
    println!("sync: backfilled {n} new post(s) from {}", short(author_did));
}

fn cmd_poll(author: Option<&str>, cycles: u32, interval_ms: i32) {
    let me = ensure_identity();
    let mut topics: Vec<String> = vec![format!("hey-v0/follow/{me}")];
    let mut following: Vec<String> = load(KV_FOLLOWING);
    if let Some(a) = author {
        if !following.contains(&a.to_string()) {
            following.push(a.to_string());
        }
    }
    for f in &following {
        topics.push(format!("hey-v0/user/{f}/posts"));
    }
    println!("polling {} topic(s) x{cycles} (every {interval_ms}ms) as {}", topics.len(), short(&me));
    let mut total_added = 0usize;
    for c in 0..cycles {
        for topic in &topics {
            let resp = match block_on(peer::recv(RecvArgs {
                topic,
                limit: 50,
                consumer_id: "hey-social-cli",
                skip_sender_id: Some(&me),
            })) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let msgs = resp
                .get("data")
                .and_then(|d| d.get("messages"))
                .or_else(|| resp.get("messages"))
                .and_then(|m| m.as_array())
                .cloned()
                .unwrap_or_default();
            for entry in msgs {
                let Some(wire) = entry
                    .get("content")
                    .or_else(|| entry.get("message"))
                    .and_then(|m| m.as_str())
                else {
                    continue;
                };
                let Some(evt) = from_wire_string(wire) else { continue };
                if verify_signed_event(&evt) != VerifyResult::Valid {
                    continue;
                }
                match evt.event_type.as_str() {
                    "posts.head" => {
                        if let Some(head) = evt.payload.get("head_cid").and_then(|c| c.as_str()) {
                            println!("[{c}] posts.head from {} head={}", short(&evt.sender_did), short(head));
                            total_added += backfill_from_index(&evt.sender_did, head);
                        }
                    }
                    "follow.request" => {
                        println!("[{c}] follow.request from {}", short(&evt.sender_did));
                    }
                    "post.create.v2" => {
                        let cid = evt.payload.get("post_cid").and_then(|c| c.as_str()).unwrap_or("");
                        println!("[{c}] post.create.v2 from {} cid={}", short(&evt.sender_did), short(cid));
                    }
                    other => println!("[{c}] {other} from {}", short(&evt.sender_did)),
                }
            }
        }
        if c + 1 < cycles {
            block_on(hey_core::plat::sleep_ms(interval_ms));
        }
    }
    println!("poll done: {total_added} post(s) backfilled into feed");
}

fn cmd_feed() {
    let me = session::current().map(|s| s.did_key).unwrap_or_default();
    let feed: Vec<FeedItem> = load(KV_FEED);
    if feed.is_empty() {
        println!("(feed empty)");
        return;
    }
    let remote = feed.iter().filter(|f| f.author != me).count();
    println!("feed: {} post(s) — {remote} REMOTE (from followed users), {} local", feed.len(), feed.len() - remote);
    for f in &feed {
        let tag = if f.author == me { "local " } else { "REMOTE" };
        println!("  [{tag}] by {}  cid={}  {:?}", short(&f.author), f.post_cid, f.caption);
    }
}

fn print_help() {
    println!(
        "hey-social-cli — headless follow + post-feed diagnostic\n\
\n\
  --base <url>     runtime API base (default http://127.0.0.1:3000)\n\
  --secret <s>     attach_secret -> mints a shell bearer (or --bearer <tok>)\n\
  --secret-file <p> read attach_secret from a runtime-coords.json (run via sudo)\n\
  --store <dir>    local CLI storage (default /tmp/hey-social-cli)\n\
\n\
Commands:\n\
  whoami                       print my did + node ticket\n\
  post <caption>               publish a post (pin to IPFS) + announce posts.head\n\
  follow <did> <ticket>        join the user's posts topic + send follow.request\n\
  poll [did] [cycles] [ms]     drain topics; on posts.head, backfill their posts\n\
  sync <did> <head_cid>        backfill directly from a known index head (no gossip)\n\
  feed                         print my feed (marks REMOTE vs local posts)\n\
\n\
Prove a new follower sees a user's posts (run on two boxes):\n\
  A$ hey-social-cli --secret <A> whoami           # note A's did + ticket\n\
  A$ hey-social-cli --secret <A> post 'hello'     # A publishes\n\
  B$ hey-social-cli --secret <B> follow <A-did> <A-ticket>\n\
  B$ hey-social-cli --secret <B> poll <A-did> 8 3000\n\
  B$ hey-social-cli --secret <B> feed             # A's post shows as REMOTE"
    );
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut base = std::env::var("HEY_BASE").unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    let mut bearer = std::env::var("HEY_BEARER").ok();
    let mut secret: Option<String> = None;
    let mut store = std::env::var("HEY_STORE").unwrap_or_else(|_| "/tmp/hey-social-cli".into());
    let mut positional: Vec<String> = Vec::new();

    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--base" => {
                i += 1;
                base = argv.get(i).cloned().unwrap_or_else(|| die("--base needs a value"));
            }
            "--bearer" => {
                i += 1;
                bearer = Some(argv.get(i).cloned().unwrap_or_else(|| die("--bearer needs a value")));
            }
            "--secret" => {
                i += 1;
                secret = Some(argv.get(i).cloned().unwrap_or_else(|| die("--secret needs a value")));
            }
            "--secret-file" => {
                i += 1;
                let p = argv.get(i).cloned().unwrap_or_else(|| die("--secret-file needs a path"));
                match secret_from_file(&p) {
                    Ok(s) => secret = Some(s),
                    Err(e) => die(&e),
                }
            }
            "--store" => {
                i += 1;
                store = argv.get(i).cloned().unwrap_or_else(|| die("--store needs a value"));
            }
            "-h" | "--help" => {
                print_help();
                return;
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    hey_core::plat::set_base(&base);
    hey_core::plat::set_store(&store);
    init(HEY_SOCIAL_CTX);

    if bearer.is_none() {
        if let Some(s) = &secret {
            match attach(s) {
                Ok(tok) => bearer = Some(tok),
                Err(e) => die(&e),
            }
        }
    }
    if let Some(tok) = &bearer {
        hey_core::plat::set_bearer(tok);
    }

    let cmd = positional.first().cloned().unwrap_or_else(|| "help".into());
    let args = &positional[1.min(positional.len())..];
    match cmd.as_str() {
        "help" => print_help(),
        "whoami" => cmd_whoami(),
        "post" => cmd_post(&args.join(" ")),
        "follow" => cmd_follow(
            args.first().unwrap_or_else(|| die("follow needs <did> <ticket>")),
            args.get(1).map(|s| s.as_str()).unwrap_or(""),
        ),
        "poll" => {
            let author = args.first().filter(|a| a.starts_with("did:key:")).map(|s| s.as_str());
            let rest: Vec<&String> = args.iter().filter(|a| !a.starts_with("did:key:")).collect();
            let cycles: u32 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(8);
            let interval: i32 = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(3000);
            cmd_poll(author, cycles, interval);
        }
        "sync" => cmd_sync(
            args.first().unwrap_or_else(|| die("sync needs <did> <head_cid>")),
            args.get(1).unwrap_or_else(|| die("sync needs <did> <head_cid>")),
        ),
        "feed" => cmd_feed(),
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            std::process::exit(2);
        }
    }
}
