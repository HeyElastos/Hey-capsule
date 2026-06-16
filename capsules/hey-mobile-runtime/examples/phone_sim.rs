//! Phone-simulator: replay EXACTLY what the Android app does after scanning the
//! desktop's friend QR — follow the link, then wait for the PQ-DM handshake to
//! complete — against the LIVE desktop hey-native app (loopback :8794).
//!
//! Run while hey-native is running: `cargo run --example phone_sim`.

use std::path::PathBuf;
use std::time::Duration;

use hey_mobile_runtime::{social, start_background, Config};

const DESKTOP_PORT: u16 = 8794; // hey-native's embedded runtime
const SIM_PORT: u16 = 8899;
const SIM_DIR: &str = "/tmp/hey-phone-sim";

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // ── 1. The link behind the desktop's QR, read from the live app ──
    let desktop_link = std::thread::spawn(|| {
        hey_core::plat::set_base(&format!("http://127.0.0.1:{DESKTOP_PORT}"));
        let store = dirs_path();
        hey_core::plat::set_store(&store);
        social::install_ctx();
        let r = rt();
        let did = r.block_on(social::whoami_did()).expect("desktop runtime not reachable on :8794");
        let link = r.block_on(social::my_friend_link()).expect("desktop friend link");
        eprintln!("[desktop] did {did}");
        link
    })
    .join()
    .unwrap();
    eprintln!("[desktop] link = {} chars", desktop_link.len());

    // ── 2. Boot the simulated phone (fresh identity, own carrier) ──
    std::fs::remove_dir_all(SIM_DIR).ok();
    hey_core::plat::set_base(&format!("http://127.0.0.1:{SIM_PORT}"));
    hey_core::plat::set_store(SIM_DIR);
    social::install_ctx();
    start_background(Config {
        data_dir: PathBuf::from(SIM_DIR),
        dist_dir: PathBuf::from("/tmp/nodist"),
        port: SIM_PORT,
        capsule: "hey-social".into(),
        identity_blob: None,
    });
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", SIM_PORT)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let r = rt();
    r.block_on(async {
        social::ensure_session().await.ok();
        social::ensure_subscriptions().await;
        eprintln!("[sim] following the desktop link (what the phone does on scan)…");
        match social::follow(&desktop_link).await {
            Ok(v) => eprintln!("[sim] follow -> {v}"),
            Err(e) => {
                eprintln!("[sim] follow FAILED: {e}");
                std::process::exit(2);
            }
        }
    });

    // ── 3. Pump like the app does; wait for the contact to go Active ──
    for tick in 0..45 {
        let done = r.block_on(async {
            social::ensure_subscriptions().await;
            let _ = social::poll_once().await;
            let contacts = hey_core::api::dms::list_contacts().await;
            let states: Vec<String> = contacts
                .iter()
                .map(|c| format!("{:?}", c.status))
                .collect();
            if tick % 5 == 0 {
                eprintln!("[sim] t+{:>3}s contact states: {states:?}", tick * 2);
            }
            states.iter().any(|s| s == "Active")
        });
        if done {
            eprintln!("RESULT: PASS — handshake completed, contact Active");
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    eprintln!("RESULT: FAIL — contact never went Active (desktop never answered)");
    std::process::exit(1);
}

fn dirs_path() -> String {
    std::env::var("XDG_DATA_HOME")
        .map(|d| format!("{d}/hey-social-native"))
        .unwrap_or_else(|_| {
            format!("{}/.local/share/hey-social-native", std::env::var("HOME").unwrap())
        })
}
