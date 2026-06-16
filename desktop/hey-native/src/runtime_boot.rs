//! Owns the embedded runtime lifecycle on desktop. Mirrors the Android JNI
//! `hey_init` + `spawn_receiver` + `spawn_dm_receiver` bodies (lib.rs android mod),
//! minus the JNI marshalling.

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender as EvSender;
use std::time::Duration;

use crate::state::UiEvent;

/// Fixed loopback port for the embedded runtime. Distinct from the Android dev
/// port (8787) so a host `cargo run --example dev` and this app can coexist.
pub const PORT: u16 = 8794;

pub struct Boot {
    pub port: u16,
    pub store: String,
}

/// Pick a stable data dir, start the in-process runtime, and block until its
/// loopback HTTP server accepts connections.
pub fn boot() -> Boot {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("hey-social-native");
    std::fs::create_dir_all(&data_dir).ok();
    log::info!("data dir {}", data_dir.display());

    // Seal the at-rest identity BEFORE the runtime touches it. Installs the OS-
    // keyring-backed storage DEK and migrates any legacy plaintext identity.json
    // (verify-before-replace). Best-effort: with no working keyring it logs and
    // leaves the prior plaintext behavior intact, so the app always launches.
    crate::at_rest::install_and_migrate(&data_dir);

    let cfg = hey_mobile_runtime::Config {
        data_dir: data_dir.clone(),
        dist_dir: PathBuf::new(), // egui draws the UI; the static mount is unused
        port: PORT,
        capsule: "hey-social".to_string(),
        identity_blob: None, // desktop: Identity::load_or_create(data_dir)
    };
    hey_mobile_runtime::start_background(cfg);
    wait_for_port(PORT);

    Boot {
        port: PORT,
        store: data_dir.to_string_lossy().into_owned(),
    }
}

fn wait_for_port(port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    for _ in 0..200 {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            log::info!("loopback runtime up on 127.0.0.1:{port}");
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    log::error!("loopback runtime did NOT come up on 127.0.0.1:{port} within ~20s");
}

/// Start the two background receivers (feed + DM). Each pins hey-core's
/// thread-locals once, owns a `current_thread` runtime, and loops forever.
pub fn start_receivers(port: u16, store: String, ctx: egui::Context, ev_tx: EvSender<UiEvent>) {
    spawn_feed_receiver(port, store.clone(), ctx.clone(), ev_tx);
    spawn_dm_receiver(port, store, ctx);
}

/// Joins my + followed topics, ingests posts/reactions/comments every ~2s, bumps
/// `feed_rev`. On a bump it pokes the UI to reload; it also drains notifications.
fn spawn_feed_receiver(port: u16, store: String, ctx: egui::Context, ev_tx: EvSender<UiEvent>) {
    std::thread::Builder::new()
        .name("hey-social-recv".into())
        .spawn(move || {
            hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
            hey_core::plat::set_store(&store);
            hey_mobile_runtime::social::install_ctx();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("feed recv runtime");
            let mut last_rev = hey_mobile_runtime::social::feed_rev();
            loop {
                // Panic-isolate the whole loop body. Inbound P2P data is
                // attacker-influenced; a panic on a malformed/hostile message must not
                // unwind out of this thread (which would silently stop ALL feed updates
                // for the process lifetime). On a caught panic we log and `continue` so
                // receive self-heals on the next tick.
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rt.block_on(async {
                        let _ = hey_mobile_runtime::social::ensure_session().await;
                        hey_mobile_runtime::social::ensure_subscriptions().await;
                        let _ = hey_mobile_runtime::social::poll_once().await;
                    });
                    let rev = hey_mobile_runtime::social::feed_rev();
                    if rev != last_rev {
                        last_rev = rev;
                        let _ = ev_tx.send(UiEvent::FeedRevBumped);
                        ctx.request_repaint();
                    }
                    if let Some(arr) = hey_mobile_runtime::social::drain_notifs().as_array() {
                        let mut any = false;
                        for n in arr {
                            let _ = ev_tx.send(UiEvent::Notif(n.clone()));
                            any = true;
                        }
                        if any {
                            ctx.request_repaint();
                        }
                    }
                }));
                if let Err(e) = res {
                    log::error!("feed receiver loop panicked: {}", panic_msg(&e));
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("spawn feed receiver");
}

/// Drives hey-core's canonical encrypted-DM/group receive loop (runs forever).
fn spawn_dm_receiver(port: u16, store: String, _ctx: egui::Context) {
    std::thread::Builder::new()
        .name("hey-dm-recv".into())
        .spawn(move || {
            hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
            hey_core::plat::set_store(&store);
            hey_mobile_runtime::social::install_ctx();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("dm recv runtime");
            // Supervised loop: `peer_receiver::run` normally loops forever, but a panic
            // on a malformed/hostile inbound encrypted message would otherwise unwind
            // out of this thread and silently stop ALL DM/group receive for the process
            // lifetime. Panic-isolate it; on a caught panic (or an unexpected normal
            // return) sleep ~1s and re-enter so receive self-heals.
            loop {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rt.block_on(async {
                        let _ = hey_mobile_runtime::social::ensure_session().await;
                        hey_core::peer_receiver::run().await; // loops forever
                    });
                }));
                match res {
                    Ok(()) => log::error!("dm receiver returned unexpectedly; restarting"),
                    Err(e) => log::error!("dm receiver loop panicked: {}", panic_msg(&e)),
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("spawn dm receiver");
}

/// Best-effort human string out of a caught panic payload (shared by both receivers).
fn panic_msg(e: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
