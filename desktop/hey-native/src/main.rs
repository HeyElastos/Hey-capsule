//! Hey — native Rust/egui desktop port of the all-in-one Hey Social app.
//! One binary: embeds hey-mobile-runtime (carrier + content + identity + social
//! API) in-process and draws the UI with egui. No JNI, no WebView, no Tauri.

mod app;
mod at_rest;
mod call_media;
mod engine;
mod icons;
mod media;
mod notify;
mod qr;
mod runtime_boot;
mod state;
mod theme;
mod util;
mod views;
mod walletops;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Pin the version-matched elastos.app relay and DROP n0's release-schedule
    // (rc/canary) relays by default — homing on an n0 relay vs the rc.1 elastos.app
    // relay can fail QUIC multipath negotiation and stop the gossip mesh forming.
    // The Connection sheet exposes this (custom relay URL + a grayed "iroh standard"
    // toggle). Honor an explicit override if the user already set the env.
    if std::env::var("ELASTOS_RELAY_ONLY").is_err() {
        std::env::set_var("ELASTOS_RELAY_ONLY", "1");
    }

    // Bring the embedded runtime up first and learn its loopback port; the UI
    // then talks to it in-process over 127.0.0.1.
    let boot = runtime_boot::boot();

    // Cross-platform baseline. The premium feel comes from the in-app self-drawn
    // large-title header, NOT from window chrome — no per-OS window flag is
    // required for the design.
    #[allow(unused_mut)]
    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 840.0]) // desktop
        .with_min_inner_size([960.0, 640.0])
        .with_maximized(true) // open fullscreen-maximized
        .with_title("Hey")
        .with_app_id("app.hey.native"); // Wayland/X11 grouping + Flatpak icon match

    // macOS ENHANCEMENT ONLY — content flows under a transparent titlebar so our
    // large-title rises to the very top edge. Guarded so Win/Linux keep their
    // normal title bar. Traffic-lights stay shown & functional, floating over the
    // sidebar's top inset (TOP_INSET, declared in app.rs). No custom window
    // controls are drawn anywhere, so there is zero per-OS chrome to maintain.
    #[cfg(target_os = "macos")]
    {
        vp = vp
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }

    let native_options = eframe::NativeOptions {
        viewport: vp,
        depth_buffer: 24, // the in-frame Verse renderer needs a depth buffer
        ..Default::default()
    };

    eframe::run_native(
        "Hey",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, boot)))),
    )
}
