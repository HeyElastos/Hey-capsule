//! Two-instance cross-device proof on ONE host: instance A posts; instance B
//! follows A (via A's friend link) and must receive A's post over the carrier.
//! Each instance is a separate runtime+carrier on its own port + data dir; they
//! mesh exactly like two phones would. Run: `cargo run --example xdev_test`.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use hey_mobile_runtime::{social, start_background, Config};

fn boot(port: u16, dir: &str) {
    std::fs::remove_dir_all(dir).ok();
    start_background(Config {
        data_dir: PathBuf::from(dir),
        dist_dir: PathBuf::from("/tmp/nodist"),
        port,
        capsule: "hey-social".into(),
        identity_blob: None,
    });
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn main() {
    let (tx, rx) = mpsc::channel::<String>();

    // ── Instance A (poster) on its own thread/base ──
    std::thread::spawn(move || {
        hey_core::plat::set_base("http://127.0.0.1:8811");
        hey_core::plat::set_store("/tmp/hey-A");
        social::install_ctx();
        boot(8811, "/tmp/hey-A");
        let r = rt();
        let link = r.block_on(async {
            social::ensure_session().await.ok();
            social::ensure_subscriptions().await;
            let link = social::my_friend_link().await.unwrap();
            // A ~250KB "photo" (multi-chunk) to exercise media-over-carrier.
            let mut photo = vec![0xABu8; 250_000];
            photo[0] = 0xFF; photo[1] = 0xD8; photo[2] = 0xFF;
            let tile = social::upload_media(&photo, "image/jpeg", "p.jpg").await.unwrap();
            let post = social::create_post("hello from A — with photo!", &format!("[{tile}]")).await.unwrap();
            eprintln!("[A] posted {} (media {})", post.get("id").unwrap(), tile.get("cid").unwrap());
            link
        });
        tx.send(link).unwrap();
        // Keep A alive + answering sync_req / re-announcing.
        loop {
            r.block_on(async {
                social::ensure_subscriptions().await;
                social::poll_once().await;
            });
            std::thread::sleep(Duration::from_secs(2));
        }
    });

    let link_a = rx.recv().unwrap();
    eprintln!("[B] got A's friend link ({} chars)", link_a.len());

    // ── Instance B (follower) on the main thread/base ──
    hey_core::plat::set_base("http://127.0.0.1:8812");
    hey_core::plat::set_store("/tmp/hey-B");
    social::install_ctx();
    boot(8812, "/tmp/hey-B");
    let r = rt();
    r.block_on(async {
        social::ensure_session().await.ok();
        eprintln!("[B] follow = {:?}", social::follow(&link_a).await);
        for i in 0..30 {
            social::ensure_subscriptions().await;
            let n = social::poll_once().await;
            let feed = social::feed(50).await.unwrap();
            let count = feed.as_array().map(|a| a.len()).unwrap_or(0);
            if count > 0 {
                eprintln!("[B] received A's post after {}s", i * 2);
                // Now confirm the MEDIA bytes arrive over the carrier too.
                let cid = feed[0]["media"][0]["cid"].as_str().unwrap_or("").to_string();
                for j in 0..20 {
                    social::poll_once().await;
                    if hey_core::runtime::content::get_bytes(&cid, None).await.is_ok() {
                        eprintln!("\n[B] RECEIVED A's PHOTO bytes cross-device (cid {cid}) after +{}s", j * 2);
                        eprintln!("\nCROSS-DEVICE POST + MEDIA OK");
                        return;
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                eprintln!("\n[B] got the post but NOT the photo bytes (media transfer stalled)");
                return;
            }
            if n > 0 {
                eprintln!("[B] ingested {n} non-post events");
            }
            eprintln!("[B] waiting… ({}s)", i * 2);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        eprintln!("\n[B] did NOT receive A's post in 60s (carrier mesh did not form on this host)");
    });
    // Exit the whole process (A loops forever) so piped stdout flushes.
    std::process::exit(0);
}
