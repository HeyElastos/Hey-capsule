//! Host smoke harness: bring the whole on-device runtime up on 127.0.0.1 so you
//! can point a desktop browser at it before any phone is involved.
//!
//!   cargo run --example dev -- ../hey-social/dist            # build dist first: cd ../hey-social && trunk build
//!   # then open http://127.0.0.1:8787/apps/hey-social/
//!
//! A second instance with a different --data-dir + --port simulates phone B, so
//! you can test cross-"device" DM/feed on one machine over the real carrier.

use std::path::PathBuf;

use hey_mobile_runtime::{run_blocking, Config};

fn main() {
    tracing_init();
    let mut args = std::env::args().skip(1);
    let dist = args.next().unwrap_or_else(|| "../hey-social/dist".into());
    let data_dir = args.next().unwrap_or_else(|| "/tmp/hey-mobile-dev".into());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(8787);

    let cfg = Config {
        data_dir: PathBuf::from(data_dir),
        dist_dir: PathBuf::from(dist),
        port,
        capsule: "hey-social".to_string(),
        identity_blob: None,
    };
    println!("open http://127.0.0.1:{port}/apps/hey-social/");
    if let Err(e) = run_blocking(cfg) {
        eprintln!("runtime error: {e:#}");
        std::process::exit(1);
    }
}

fn tracing_init() {
    // run_blocking installs the ring-buffer logger (logbuf), which also prints
    // to stderr on host — so no separate logger init is needed here.
}
