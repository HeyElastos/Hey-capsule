//! hey-chat-cli — headless diagnostic for the cross-runtime Hey DM flow.
//!
//! Runs the EXACT hey-core invite/handshake/receive code path the wasm apps
//! run, but natively over the runtime's loopback provider API. Create an
//! invite on box A, accept it on box B, then poll on A and watch precisely
//! where the handshake does (or doesn't) cross.
//!
//! Auth: the runtime's loopback `/api/auth/attach {secret, scope:"shell"}`
//! mints a shell-scope bearer; pass the bearer with `--bearer`/`$HEY_BEARER`,
//! or pass the raw `attach_secret` with `--secret` and let the CLI attach.
//!
//! Identity: `adopt_provider_identity()` adopts the runtime's "hey"-namespace
//! identity (the wallet model — keys stay in the runtime identity provider),
//! exactly like the app's wallet sign-in. No passkey, no local seed.

use hey_core::api::dms::{self, IdentityMode};
use hey_core::api::outbox;
use hey_core::ctx::{init, CapsuleCtx};
use hey_core::runtime::peer;
use hey_core::session;
use serde_json::{json, Value};
use std::future::Future;

// Per-capsule identity — byte-identical to capsules/hey-chat/src/main.rs so the
// CLI shares the hey-chat storage namespace + "hey" signing identity.
const HEY_CHAT_CTX: CapsuleCtx = CapsuleCtx {
    capsule_id: "hey-chat",
    private_namespace: "HeyChat",
    session_key: "hey-chat-session",
    welcomed_key: "hey-chat-welcomed",
    session_redeemed_key: "hey-chat-redeemed",
    home_launch_token_key: "hey-chat-home-token",
    runtime_token_key: "hey-chat-runtime-token",
    token_store_key: "hey-chat-capability-tokens",
    route_mode_key: "hey-chat-storage-route-mode",
    boot_capabilities: &[
        ("elastos://peer/*", "message"),
        ("elastos://blobs/*", "write"),
        ("elastos://did/*", "read"),
    ],
};

// ── Minimal single-thread executor ───────────────────────────────────────
// Every native leaf op (plat::http, plat::sleep_ms) blocks synchronously, so
// no future ever returns Pending — a noop waker + tight poll is correct and
// never busy-spins.
fn block_on<F: Future>(fut: F) -> F::Output {
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn raw() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            raw()
        }
        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, no_op, no_op, no_op),
        )
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

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
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

/// Adopt the runtime's "hey" identity into a native session (idempotent).
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

fn my_ticket() -> Option<String> {
    block_on(peer::my_ticket())
}

fn topic_neighbor(topic: &str) -> (bool, Value) {
    let has = block_on(peer::has_topic_peer(topic));
    let list = block_on(peer::list_topic_peers(topic)).unwrap_or(Value::Null);
    (has, list)
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut base = std::env::var("HEY_BASE").unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    let mut bearer = std::env::var("HEY_BEARER").ok();
    let mut secret: Option<String> = None;
    let mut store = std::env::var("HEY_STORE").unwrap_or_else(|_| "/tmp/hey-cli".into());
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

    // Wire the platform shim BEFORE any engine call.
    hey_core::plat::set_base(&base);
    hey_core::plat::set_store(&store);
    init(HEY_CHAT_CTX);

    // Resolve a bearer: explicit > attach(secret).
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
        "ticket" => cmd_ticket(),
        "invite" => cmd_invite(args.first().map(|s| s.as_str()).unwrap_or("CLI invite")),
        "accept" => cmd_accept(args.first().unwrap_or_else(|| die("accept needs <token>"))),
        "decode" => cmd_decode(args.first().unwrap_or_else(|| die("decode needs <token>"))),
        "poll" => {
            let cycles: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(6);
            let interval: i32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3000);
            cmd_poll(cycles, interval);
        }
        "contacts" => cmd_contacts(),
        "create-group" => cmd_create_group(
            args.first().unwrap_or_else(|| die("create-group needs <name> <did>...")),
            args.get(1..).map(|a| a.to_vec()).unwrap_or_default(),
        ),
        "groups" => cmd_groups(),
        "send-group" => cmd_send_group(
            args.first().unwrap_or_else(|| die("send-group needs <gid> <text>")),
            &args.get(1..).map(|a| a.join(" ")).unwrap_or_default(),
        ),
        "group-read" => cmd_group_read(args.first().unwrap_or_else(|| die("group-read needs <gid>"))),
        "att-test" => cmd_att_test(args.first().and_then(|s| s.parse().ok()).unwrap_or(768usize)),
        "group-fetch" => cmd_group_fetch(args.first().unwrap_or_else(|| die("group-fetch needs <gid>"))),
        "send-group-file" => cmd_send_group_file(
            args.first().unwrap_or_else(|| die("send-group-file needs <gid> [text] [kb]")),
            args.get(1).map(|s| s.as_str()).unwrap_or(""),
            args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8usize),
        ),
        "health" => cmd_health(),
        "delete" => cmd_delete(args.first().unwrap_or_else(|| die("delete needs <did>"))),
        "topics" => cmd_topics(),
        "send" => cmd_send(
            args.first().unwrap_or_else(|| die("send needs <did> <text>")),
            &args.get(1..).map(|a| a.join(" ")).unwrap_or_default(),
        ),
        "send-file" => cmd_send_file(
            args.first().unwrap_or_else(|| die("send-file needs <did> [kb]")),
            args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5usize),
        ),
        "recv-file" => cmd_recv_file(args.first().unwrap_or_else(|| die("recv-file needs <did>"))),
        "peer" => cmd_peer(
            args.first().unwrap_or_else(|| die("peer needs <op> [json]")),
            args.get(1).map(|s| s.as_str()),
        ),
        other => die(&format!("unknown command '{other}' (try: help)")),
    }
}

fn print_help() {
    eprintln!(
        "hey-chat-cli — cross-runtime Hey DM diagnostic\n\
\n\
GLOBAL  --base <url>   runtime API base (default http://127.0.0.1:3000)\n\
        --bearer <t>   shell-scope bearer ($HEY_BEARER)\n\
        --secret <hex> attach_secret; CLI mints the bearer via /api/auth/attach\n\
        --store <dir>  local storage root for contacts/outbox/session ($HEY_STORE, default /tmp/hey-cli)\n\
\n\
COMMANDS\n\
  whoami                adopt the runtime 'hey' identity; print did + node ticket\n\
  ticket                print this runtime's peer node id + gossip ticket\n\
  invite [label]        generate an invite link (prints the hey-invite: token)\n\
  accept <token>        accept an invite; sends the handshake to the inviter's queue\n\
  decode <token>        decode + print an invite link (no side effects)\n\
  poll [cycles] [ms]    run the receive loop with per-topic neighbor tracing\n\
  contacts              dump local contact records (JSON)\n\
  create-group <name> <did>...  create a group from active contacts\n\
  groups                list groups\n\
  send-group <gid> <text>       send a message to a group (fan-out)\n\
  group-read <gid>      print a group conversation\n\
  health                carrier status pill data (online / peers / queued)\n\
  delete <did>          delete a conversation + ALL its local data\n\
  topics                list v2 DM topics + per-topic neighbor status\n\
  send <did> <text>     send a DM to an established contact\n\
  peer <op> [json]      raw provider_call(\"peer\", op, body) passthrough\n"
    );
}

fn cmd_whoami() {
    let did = ensure_identity();
    println!("did       : {did}");
    match my_ticket() {
        Some(t) => println!("node_ticket: {t}"),
        None => println!("node_ticket: <none> (peer get_ticket failed)"),
    }
}

fn cmd_ticket() {
    match block_on(peer::get_ticket()) {
        Ok(v) => println!("{}", pretty(&v)),
        Err(e) => die(&format!("get_ticket: {e}")),
    }
}

fn cmd_invite(label: &str) {
    let did = ensure_identity();
    println!("my did     : {did}");
    match my_ticket() {
        Some(t) => println!("my ticket  : {}…", &t[..t.len().min(48)]),
        None => println!("my ticket  : <none>"),
    }
    match block_on(dms::generate_invite(label, IdentityMode::Regular)) {
        Ok(token) => {
            // Decode to surface the queue topic for cross-checking.
            if let Ok(inv) = dms::decode_invite_link(&token) {
                println!("queue      : {}", inv.queue);
                println!("topic      : q/{}", inv.queue);
                println!(
                    "node_ticket: {}",
                    inv.node_ticket.as_deref().unwrap_or("<none>")
                );
            }
            println!("\n{token}");
        }
        Err(e) => die(&format!("generate_invite: {e}")),
    }
}

fn cmd_decode(token: &str) {
    match dms::decode_invite_link(token) {
        Ok(inv) => {
            println!("did        : {}", inv.did);
            println!("name       : {}", inv.name);
            println!("queue      : {}", inv.queue);
            println!("topic      : q/{}", inv.queue);
            println!(
                "node_ticket: {}",
                inv.node_ticket.as_deref().unwrap_or("<none>")
            );
            println!("expires_at : {}", inv.expires_at);
        }
        Err(e) => die(&format!("decode_invite_link: {e}")),
    }
}

fn cmd_accept(token: &str) {
    let did = ensure_identity();
    println!("my did     : {did}");
    let inv = match dms::decode_invite_link(token) {
        Ok(v) => v,
        Err(e) => die(&format!("decode_invite_link: {e}")),
    };
    let inviter_topic = format!("q/{}", inv.queue);
    println!("inviter did: {}", inv.did);
    println!("inviter q  : {inviter_topic}");
    println!(
        "inviter tkt: {}",
        inv.node_ticket.as_deref().unwrap_or("<none>")
    );
    println!("\n-- accept_invite (joins inviter queue, sends handshake) --");
    match block_on(dms::accept_invite(token, IdentityMode::Regular)) {
        Ok(peer_did) => println!("accepted, peer did: {peer_did}"),
        Err(e) => die(&format!("accept_invite: {e}")),
    }
    // Did the handshake actually go onto a topic with a live neighbor?
    let (has, list) = topic_neighbor(&inviter_topic);
    println!("\ninviter topic neighbor present: {has}");
    println!("list_topic_peers: {}", pretty(&list));
    println!("outbox pending : {}", block_on(outbox::pending_count()));
    println!(
        "\n→ if neighbor=false the handshake went into an empty mesh (the bug);\n  run `poll` here to watch the outbox retry re-graft + redeliver."
    );
}

fn cmd_poll(cycles: u32, interval_ms: i32) {
    let did = ensure_identity();
    println!("polling as {did}  ({cycles} cycles, {interval_ms}ms)\n");
    for c in 0..cycles {
        println!("──────── cycle {} ────────", c + 1);
        let topics = block_on(dms::my_v2_topics());
        if topics.is_empty() {
            println!("  (no v2 topics yet — no contacts/invites on this box)");
        }
        for (topic, consumer, boot) in &topics {
            let (has_before, _) = topic_neighbor(topic);
            // Mirror peer_receiver::poll_once: ensure join, gate on neighbor.
            let _ = block_on(peer::join_topic_with(topic, boot));
            if !boot.is_empty() && !has_before {
                let confirmed = block_on(peer::wait_for_topic_peers(topic, boot));
                println!(
                    "  {topic}  boot={}  neighbor_before=false wait_for_peers={confirmed}",
                    boot.len()
                );
            }
            let (has, list) = topic_neighbor(topic);
            let peer_ids = list
                .get("data")
                .and_then(|d| d.get("peers"))
                .or_else(|| list.get("peers"))
                .and_then(|p| p.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            // Drain the queue (feeds receive_v2_wire just like the real loop).
            let recv = block_on(peer::recv(peer::RecvArgs {
                topic,
                limit: 50,
                consumer_id: consumer,
                skip_sender_id: None,
            }))
            .unwrap_or(Value::Null);
            let msgs = recv
                .get("data")
                .and_then(|d| d.get("messages"))
                .or_else(|| recv.get("messages"))
                .and_then(|m| m.as_array())
                .cloned()
                .unwrap_or_default();
            println!(
                "  {topic}  neighbor={has} peers={peer_ids} recv={} msgs",
                msgs.len()
            );
            for entry in &msgs {
                if let Some(wire) = entry
                    .get("content")
                    .or_else(|| entry.get("message"))
                    .and_then(|m| m.as_str())
                {
                    // Reassemble fragments exactly like peer_receiver does.
                    match hey_core::api::frag::reassemble(wire) {
                        Some(full) => match block_on(dms::receive_v2_wire(topic, &full)) {
                            Ok(()) => println!("    ✓ ingested wire ({} B)", full.len()),
                            Err(e) => println!("    ✗ receive_v2_wire: {e}"),
                        },
                        None => println!("    … buffered fragment (set incomplete)"),
                    }
                }
            }
        }
        block_on(outbox::flush());
        println!("  outbox pending after flush: {}", block_on(outbox::pending_count()));
        // Contact status snapshot.
        for c in block_on(dms::list_contacts()) {
            println!(
                "  contact {} [{:?}] inq={:?} theirq={:?} tkt={}",
                short(&c.did),
                c.status,
                c.my_inbound_queue.as_deref().map(short),
                c.their_inbound_queue.as_deref().map(short),
                c.peer_ticket.is_some()
            );
        }
        if c + 1 < cycles {
            block_on(hey_core::plat::sleep_ms(interval_ms));
        }
    }
}

fn cmd_create_group(name: &str, member_dids: Vec<String>) {
    let _ = ensure_identity();
    if member_dids.is_empty() {
        die("create-group needs at least one member did");
    }
    match block_on(dms::create_group(name, member_dids)) {
        Ok(gid) => println!("created group '{name}' -> {gid}"),
        Err(e) => die(&format!("create_group: {e}")),
    }
}

fn cmd_groups() {
    let _ = ensure_identity();
    let groups = block_on(dms::list_groups());
    if groups.is_empty() {
        println!("(no groups)");
    }
    for g in groups {
        let members: Vec<String> = g.members.iter().map(|m| short(&m.did)).collect();
        println!(
            "{}  \"{}\"  [{} members: {}]  unread={}  last={:?}",
            short(&g.id),
            g.name,
            g.members.len(),
            members.join(", "),
            g.unread,
            g.last_preview
        );
        println!("  full id: {}", g.id);
    }
}

fn cmd_send_file(did: &str, kb: usize) {
    let _ = ensure_identity();
    let bytes = vec![0x37u8; kb.max(1) * 1024];
    let att = match block_on(dms::upload_attachment("hey.bin", "application/octet-stream", &bytes)) {
        Ok(a) => a,
        Err(e) => die(&format!("upload_attachment ({kb}KB): {e}")),
    };
    println!(
        "uploaded {}KB: inline={} cid={}",
        kb,
        att.inline_b64.is_some(),
        short(&att.cid)
    );
    match block_on(dms::send_message_with_attachments(did, "", vec![att])) {
        Ok(_) => println!("sent file to {did}"),
        Err(e) => die(&format!("send_message_with_attachments: {e}")),
    }
    println!("outbox pending: {}", block_on(outbox::pending_count()));
}

fn cmd_recv_file(did: &str) {
    let _ = ensure_identity();
    let conv = block_on(dms::read_conversation(did));
    let att = conv.iter().rev().find_map(|m| m.attachments.first().cloned());
    match att {
        Some(a) => {
            println!(
                "attachment: name={} size={} inline={} chunks={} cid={}",
                a.name,
                a.size,
                a.inline_b64.is_some(),
                if a.chunks.is_empty() { 1 } else { a.chunks.len() },
                short(&a.cid)
            );
            match block_on(dms::fetch_attachment(&a)) {
                Ok(bytes) => println!(
                    "  {} fetched + decrypted {} bytes (expected {}) — {}",
                    if bytes.len() as u64 == a.size { "✓" } else { "✗" },
                    bytes.len(),
                    a.size,
                    if bytes.len() as u64 == a.size { "MATCH" } else { "MISMATCH" }
                ),
                Err(e) => println!("  ✗ fetch: {e}"),
            }
        }
        None => println!("no attachment in conversation with {did}"),
    }
}

fn cmd_att_test(kb: usize) {
    let _ = ensure_identity();
    let bytes = vec![0x37u8; kb * 1024];
    let nchunks = (kb * 1024).div_ceil(256 * 1024);
    println!("uploading {kb}KB ({nchunks} chunk(s) @256KB)…");
    let att = match block_on(dms::upload_attachment("t.bin", "application/octet-stream", &bytes)) {
        Ok(a) => a,
        Err(e) => die(&format!("upload: {e}")),
    };
    println!(
        "  uploaded: chunks={} cid={}",
        if att.chunks.is_empty() { 1 } else { att.chunks.len() },
        short(&att.cid)
    );
    match block_on(dms::fetch_attachment(&att)) {
        Ok(b) => println!(
            "  {} fetch+decrypt {} bytes (expected {}){}",
            if b.len() == kb * 1024 && b == bytes { "✓" } else { "✗" },
            b.len(),
            kb * 1024,
            if b.len() == kb * 1024 && b == bytes { " — MATCH" } else { " — MISMATCH" }
        ),
        Err(e) => println!("  ✗ fetch: {e}"),
    }
}

fn cmd_group_fetch(gid: &str) {
    let _ = ensure_identity();
    let conv = block_on(dms::read_group_conversation(gid));
    let att = conv.iter().rev().find_map(|m| m.attachments.first().cloned());
    match att {
        Some(a) => {
            println!(
                "attachment: name={} size={} chunks={} cid={}",
                a.name,
                a.size,
                if a.chunks.is_empty() { 1 } else { a.chunks.len() },
                short(&a.cid)
            );
            match block_on(dms::fetch_attachment(&a)) {
                Ok(bytes) => println!(
                    "  ✓ fetched + decrypted {} bytes (expected {}){}",
                    bytes.len(),
                    a.size,
                    if bytes.len() as u64 == a.size { " — MATCH" } else { " — MISMATCH" }
                ),
                Err(e) => println!("  ✗ fetch: {e}"),
            }
        }
        None => println!("no attachment in group {gid}"),
    }
}

fn cmd_send_group_file(gid: &str, text: &str, kb: usize) {
    let _ = ensure_identity();
    // Synthetic attachment of `kb` KiB (default 8) to reproduce the group
    // file-attachment path without a real picker.
    let bytes = vec![0x42u8; kb.max(1) * 1024];
    let att = match block_on(dms::upload_attachment("test.png", "image/png", &bytes)) {
        Ok(a) => a,
        Err(e) => die(&format!("upload_attachment ({kb}KB): {e}")),
    };
    println!("uploaded att: cid={} size={}", short(&att.cid), kb * 1024);
    match block_on(dms::send_group_message_with_attachments(gid, text, vec![att])) {
        Ok(_) => println!("sent group file to {gid}"),
        Err(e) => die(&format!("send_group_message_with_attachments: {e}")),
    }
    println!("outbox pending: {}", block_on(outbox::pending_count()));
}

fn cmd_send_group(gid: &str, text: &str) {
    let _ = ensure_identity();
    if text.is_empty() {
        die("send-group needs message text");
    }
    match block_on(dms::send_group_message(gid, text)) {
        Ok(_) => println!("sent to group {gid}"),
        Err(e) => die(&format!("send_group_message: {e}")),
    }
    println!("outbox pending: {}", block_on(outbox::pending_count()));
}

fn cmd_group_read(gid: &str) {
    let _ = ensure_identity();
    let conv = block_on(dms::read_group_conversation(gid));
    if conv.is_empty() {
        println!("(no messages in group {gid})");
    }
    for m in conv {
        let who = if m.mine {
            "me".to_string()
        } else if !m.sender_name.is_empty() {
            m.sender_name.clone()
        } else {
            "?".to_string()
        };
        println!("  [{who}] {}", m.text);
    }
}

fn cmd_health() {
    let h = block_on(peer::carrier_health());
    let q = block_on(outbox::pending_count());
    let pill = if !h.online {
        "🔴 Offline — server may need a restart".to_string()
    } else if h.peer_count == 0 {
        "🟡 Connecting…".to_string()
    } else {
        format!("🟢 Online · {} peer(s)", h.peer_count)
    };
    let qtxt = if q > 0 { format!(" · {q} queued") } else { String::new() };
    println!("{pill}{qtxt}");
    println!("  online  : {}", h.online);
    println!("  node_id : {}", if h.node_id.is_empty() { "<none>" } else { &h.node_id });
    println!("  peers   : {}", h.peer_count);
    println!("  queued  : {q}");
}

fn cmd_delete(did: &str) {
    let _ = ensure_identity();
    match block_on(dms::delete_conversation(did)) {
        Ok(()) => println!("deleted conversation + all local data for {did}"),
        Err(e) => die(&format!("delete_conversation: {e}")),
    }
}

fn cmd_contacts() {
    let _ = ensure_identity();
    let list = block_on(dms::list_contacts());
    match serde_json::to_value(&list) {
        Ok(v) => println!("{}", pretty(&v)),
        Err(e) => die(&format!("serialize contacts: {e}")),
    }
}

fn cmd_topics() {
    let _ = ensure_identity();
    println!("-- my_v2_topics --");
    for (topic, consumer, boot) in block_on(dms::my_v2_topics()) {
        let (has, list) = topic_neighbor(&topic);
        let n = list
            .get("data")
            .and_then(|d| d.get("peers"))
            .or_else(|| list.get("peers"))
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        println!("  {topic}  neighbor={has} peers={n} boot={} consumer={}", boot.len(), short(&consumer));
    }
    println!("\n-- provider list_peers --");
    println!("{}", pretty(&block_on(peer::list_peers()).unwrap_or(Value::Null)));
}

fn cmd_send(did: &str, text: &str) {
    let _ = ensure_identity();
    if text.is_empty() {
        die("send needs message text");
    }
    match block_on(dms::send_message(did, text)) {
        Ok(_) => println!("sent to {did}"),
        Err(e) => die(&format!("send_message: {e}")),
    }
    let topic = block_on(dms::my_v2_topics())
        .into_iter()
        .find(|(_, _, b)| !b.is_empty());
    if let Some((t, _, _)) = topic {
        let (has, _) = topic_neighbor(&t);
        println!("a send topic {t} neighbor={has}");
    }
    println!("outbox pending: {}", block_on(outbox::pending_count()));
}

fn cmd_peer(op: &str, body: Option<&str>) {
    let body: Value = match body {
        Some(s) => serde_json::from_str(s).unwrap_or_else(|e| die(&format!("bad json body: {e}"))),
        None => json!({}),
    };
    match block_on(hey_core::runtime::provider_call("peer", op, body)) {
        Ok(v) => println!("{}", pretty(&v)),
        Err(e) => die(&format!("peer/{op}: {e}")),
    }
}

fn short(s: &str) -> String {
    if s.len() > 14 {
        format!("{}…{}", &s[..6], &s[s.len() - 6..])
    } else {
        s.to_string()
    }
}
