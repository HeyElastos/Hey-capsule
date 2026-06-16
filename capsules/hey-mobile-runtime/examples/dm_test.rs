//! Two-peer DM ground-truth proof, ONE PEER PER PROCESS.
//!
//! `hey-mobile-runtime` keeps the seed-backed identity in a PROCESS-GLOBAL
//! `crate::IDENTITY` OnceLock (the phone is one runtime per process), so two
//! runtimes in ONE process would share ONE identity — B could never accept A's
//! invite ("that's your own invite link"). The faithful fix is two OS processes:
//! each gets its own IDENTITY + its own embedded iroh carrier = two real peers
//! that mesh exactly like two phones.
//!
//!   role A: cargo run --example dm_test -- A 8821 /tmp/mesh-A
//!   role B: cargo run --example dm_test -- B 8822 /tmp/mesh-B
//!
//! They rendezvous through files under /tmp/mesh-rdv:
//!   A writes invite.txt + did_A.txt; B reads them, accepts, writes did_B.txt.
//!   A reads did_B.txt, waits for the contact to go Active, sends the DM.
//!   B drains its v2 queue and confirms the message arrived.
//!
//! The DM invite/handshake/message path signs + ratchets with the RAW 32-byte
//! seed synchronously, so each process installs a FULL local-seed session read
//! from the runtime's own identity.json (the phone has the seed locally). The
//! social/feed path can sign via the provider with an empty seed; DMs cannot.

use std::path::PathBuf;
use std::time::Duration;

use hey_mobile_runtime::{social, start_background, Config};

use hey_core::api::dms::{self, ContactStatus};
use hey_core::api::{frag, outbox};
use hey_core::runtime::peer;
use hey_core::session;
use serde_json::Value;

const RDV: &str = "/tmp/mesh-rdv";

fn rdv_write(name: &str, val: &str) {
    std::fs::create_dir_all(RDV).ok();
    std::fs::write(format!("{RDV}/{name}"), val).expect("rdv write");
}
fn rdv_read(name: &str) -> Option<String> {
    std::fs::read_to_string(format!("{RDV}/{name}")).ok().filter(|s| !s.trim().is_empty())
}
fn rdv_wait(name: &str, secs: u64) -> Option<String> {
    for _ in 0..(secs * 2) {
        if let Some(v) = rdv_read(name) {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

fn boot(port: u16, dir: &str) {
    std::fs::remove_dir_all(dir).ok();
    start_background(Config {
        data_dir: PathBuf::from(dir),
        dist_dir: PathBuf::from("/tmp/nodist"),
        port,
        capsule: "hey-chat".into(),
        identity_blob: None,
    });
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("runtime on :{port} never bound");
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

/// Install a FULL local-seed session from the runtime's on-disk identity.json so
/// the DM path can sign/ratchet synchronously with the raw seed. did_key comes
/// from the provider whoami so it always matches what the invite advertises.
fn install_full_session(data_dir: &str, did: &str) {
    use base64::Engine;
    let raw = std::fs::read(format!("{data_dir}/identity.json")).expect("read identity.json");
    let blob: Value = serde_json::from_slice(&raw).expect("parse identity.json");
    let b64 = base64::engine::general_purpose::STANDARD;
    let seed = b64
        .decode(blob["seed_b64"].as_str().expect("seed_b64"))
        .expect("decode seed_b64");
    let auth_key_hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    session::set(&session::Session {
        auth_key_hex,
        did_key: did.to_string(),
        name: String::new(),
        ml_kem_secret_b64: blob["ml_kem_secret_b64"].as_str().unwrap_or("").to_string(),
        ml_kem_public_b64: blob["ml_kem_public_b64"].as_str().unwrap_or("").to_string(),
    });
}

/// One turn of the DM receive loop — byte-for-byte hey_core::peer_receiver. Prints
/// every per-pair topic's neighbor state so the mesh is observable. Returns true
/// if ANY per-pair topic currently has a confirmed neighbor.
async fn dm_pump(tag: &str) -> bool {
    let mut any_neighbor = false;
    for (topic, consumer, boot) in dms::my_v2_topics().await {
        let _ = peer::join_topic_with(&topic, &boot).await;
        if !boot.is_empty() && !peer::has_topic_peer(&topic).await {
            let confirmed = peer::wait_for_topic_peers(&topic, &boot).await;
            eprintln!("[{tag}] {topic} boot={} wait_for_peers={confirmed}", boot.len());
        }
        let has = peer::has_topic_peer(&topic).await;
        let list = peer::list_topic_peers(&topic).await.unwrap_or(Value::Null);
        let n = list
            .get("data")
            .and_then(|d| d.get("peers"))
            .or_else(|| list.get("peers"))
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        any_neighbor |= has || n > 0;
        let recv = peer::recv(peer::RecvArgs {
            topic: &topic,
            limit: 50,
            consumer_id: &consumer,
            skip_sender_id: None,
        })
        .await
        .unwrap_or(Value::Null);
        let msgs = recv
            .get("data")
            .and_then(|d| d.get("messages"))
            .or_else(|| recv.get("messages"))
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        eprintln!("[{tag}] {topic} neighbor={has} peers={n} recv={} msgs", msgs.len());
        for entry in &msgs {
            if let Some(wire) = entry
                .get("content")
                .or_else(|| entry.get("message"))
                .and_then(|m| m.as_str())
            {
                match frag::reassemble(wire) {
                    Some(full) => match dms::receive_v2_wire(&topic, &full).await {
                        Ok(()) => eprintln!("[{tag}]    ingested wire ({} B)", full.len()),
                        Err(e) => eprintln!("[{tag}]    receive_v2_wire ERR: {e}"),
                    },
                    None => eprintln!("[{tag}]    buffered fragment"),
                }
            }
        }
    }
    outbox::flush().await;
    any_neighbor
}

async fn contact_active(peer_did: &str) -> bool {
    dms::list_contacts()
        .await
        .iter()
        .any(|c| c.did == peer_did && c.status == ContactStatus::Active)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut a = std::env::args().skip(1);
    let role = a.next().unwrap_or_default();
    let port: u16 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let dir = a.next().unwrap_or_default();
    if !matches!(role.as_str(), "A" | "B") || port == 0 || dir.is_empty() {
        eprintln!("usage: dm_test <A|B> <port> <data_dir>");
        std::process::exit(64);
    }

    hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
    hey_core::plat::set_store(&dir);
    social::install_ctx();
    boot(port, &dir);

    let r = rt();
    let did = r.block_on(async {
        social::ensure_session().await.ok();
        social::whoami_did().await.unwrap()
    });
    install_full_session(&dir, &did);
    eprintln!("[{role}] up on :{port}  did={did}");

    if role == "A" {
        // Clean slate on the inviter run so a stale rendezvous can't cross runs.
        std::fs::remove_dir_all(RDV).ok();
        let invite = r.block_on(social::chat_gen_invite("DM ground-truth")).expect("gen invite");
        rdv_write("did_A.txt", &did);
        rdv_write("invite.txt", &invite);
        eprintln!("[A] published invite ({} chars) + did", invite.len());

        let did_b = rdv_wait("did_B.txt", 90).expect("B never published its did");
        eprintln!("[A] learned B's did = {did_b}");

        let mut sent = false;
        let mut neighbor_ever = false;
        for tick in 0..120 {
            neighbor_ever |= r.block_on(dm_pump("A"));
            let active = r.block_on(contact_active(&did_b));
            if active && !sent {
                eprintln!("[A] t+{}s contact B ACTIVE — sending DM", tick * 2);
                let res = r.block_on(dms::send_message(&did_b, "ground-truth DM: hello B from A"));
                eprintln!("[A] send_message -> {res:?}");
                r.block_on(outbox::flush());
                rdv_write("sent.txt", "1");
                sent = true;
            }
            if rdv_read("delivered.txt").is_some() {
                eprintln!("[A] B confirmed delivery");
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        eprintln!("[A] neighbor_ever={neighbor_ever} sent={sent}");
        std::process::exit(0);
    } else {
        // role B
        let invite = rdv_wait("invite.txt", 90).expect("A never published an invite");
        let did_a = rdv_wait("did_A.txt", 90).expect("A never published its did");
        eprintln!("[B] got A's invite + did_A={did_a}");
        match r.block_on(social::chat_accept_invite(&invite)) {
            Ok(p) => eprintln!("[B] accept_invite OK, peer did = {p}"),
            Err(e) => {
                eprintln!("[B] accept_invite FAILED: {e}");
                std::process::exit(2);
            }
        }
        rdv_write("did_B.txt", &did);
        eprintln!("[B] published my did; pumping for A's DM…");

        let mut neighbor_ever = false;
        let mut delivered = false;
        for tick in 0..120 {
            neighbor_ever |= r.block_on(dm_pump("B"));
            let conv = r.block_on(dms::read_conversation(&did_a));
            let incoming: Vec<String> = conv
                .iter()
                .filter(|m| !m.mine && !m.text.is_empty())
                .map(|m| m.text.clone())
                .collect();
            if tick % 3 == 0 {
                let active = r.block_on(contact_active(&did_a));
                eprintln!(
                    "[B] t+{}s contact_active={active} neighbor_ever={neighbor_ever} incoming={incoming:?}",
                    tick * 2
                );
            }
            if incoming.iter().any(|t| t.contains("ground-truth DM")) {
                eprintln!("\n[B] RECEIVED A's DM: {incoming:?}");
                rdv_write("delivered.txt", "1");
                delivered = true;
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        eprintln!("\n==================== RESULT (B) ====================");
        eprintln!("MESHED (per-pair neighbor seen on B): {neighbor_ever}");
        eprintln!("DELIVERED (B received A's DM):         {delivered}");
        eprintln!("===================================================");
        std::process::exit(if delivered { 0 } else { 1 });
    }
}
