// ConnBadge — global connection-mode badge.
//
// Polls the carrier's `peer_paths` op (iroh 1.0 endpoint.remote_info, carrier
// PATCH 0019) and shows whether your live P2P links are DIRECT (relay-free
// peer-to-peer) or via a RELAY (forwarded when NAT blocks a direct path).
// End-to-end post-quantum encrypted either way — the relay only sees
// ciphertext. Copied verbatim from hey-chat so both apps surface the same
// indicator.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;

#[component]
pub fn ConnBadge() -> impl IntoView {
    let label = RwSignal::new(String::new());
    Effect::new(move |_| {
        spawn_local(async move {
            loop {
                let (mut direct, mut relay) = (0u32, 0u32);
                if let Ok(v) = hey_core::runtime::peer::peer_paths().await {
                    if let Some(paths) = v["data"]["paths"].as_object() {
                        for val in paths.values() {
                            let k = val.as_str().unwrap_or("");
                            if k.starts_with("DIRECT") {
                                direct += 1;
                            } else if k == "RELAY" {
                                relay += 1;
                            }
                        }
                    }
                }
                let txt = if direct == 0 && relay == 0 {
                    String::new()
                } else if relay == 0 {
                    "🔒 Direct P2P".to_string()
                } else if direct == 0 {
                    "↪ via Relay".to_string()
                } else {
                    format!("🔒 {direct} direct · ↪ {relay} relay")
                };
                label.set(txt);
                wait_ms(5000).await;
            }
        });
    });
    view! {
        <span
            class="msgr-conn-badge"
            style="font-size:11px;font-weight:600;padding:3px 9px;border-radius:999px;background:rgba(52,225,212,0.14);color:#2bb9ad;white-space:nowrap;align-self:center;margin-left:8px"
            title="How your peers are reached: 🔒 direct = relay-free peer-to-peer; ↪ relay = forwarded only when NAT blocks a direct path. End-to-end post-quantum encrypted either way — the relay sees only ciphertext."
        >
            {move || label.get()}
        </span>
    }
}

async fn wait_ms(ms: i32) {
    let win = web_sys::window().unwrap();
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}
