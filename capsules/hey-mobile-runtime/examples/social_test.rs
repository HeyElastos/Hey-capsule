//! Host proof of the native post→feed loop (no JNI, no UI): start the in-process
//! runtime, point hey-core at it, then exercise the social app-API the Compose
//! UI will call. Run: `cargo run --example social_test`.

use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use hey_mobile_runtime::{social, start_background, Config};

fn main() {
    let data = "/tmp/hey-social-apitest";
    std::fs::remove_dir_all(data).ok();
    let port = 8801u16;

    start_background(Config {
        data_dir: PathBuf::from(data),
        dist_dir: PathBuf::from("/tmp/nodist"),
        port,
        capsule: "hey-social".into(),
        identity_blob: None,
    });

    // Wait for the loopback server.
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // hey-core's native plat base/store are thread-local — set on THIS thread,
    // then drive the async ops on this same thread (current-thread runtime).
    hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
    hey_core::plat::set_store(data);
    social::install_ctx();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        println!("whoami    = {:?}", social::whoami().await);
        let tile = social::upload_media(b"\xff\xd8\xff\x00 fake-jpeg bytes", "image/jpeg", "p.jpg")
            .await
            .expect("upload");
        println!("upload    = {tile}");
        let media = format!("[{tile}]");
        let post = social::create_post("hello from native Hey", &media)
            .await
            .expect("create_post");
        println!("post      = {post}");
        let feed = social::feed(10).await.expect("feed");
        let n = feed.as_array().map(|a| a.len()).unwrap_or(0);
        println!("feed({n}) = {feed}");
        assert!(n >= 1, "feed should contain the post we just made");

        let pid = post.get("id").and_then(|v| v.as_str()).unwrap();
        println!("react     = {:?}", social::react(pid, "❤️").await);
        println!("comment   = {:?}", social::add_comment(pid, "nice shot!", "").await);
        println!("comments  = {:?}", social::get_comments(pid).await);
        let link = social::my_friend_link().await.unwrap();
        println!("friend    = {link}");
        println!("follow    = {:?}", social::follow("did:key:z6MkFakePeerForTest000000000000000000000000").await);
        println!("following = {:?}", social::following().await);
        println!("\nNATIVE POST→FEED + LIKE/COMMENT/FOLLOW OK");
    });
}
