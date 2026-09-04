//! The only file that knows the runtime's wire shape.
//!
//! A thin shim over `hey_core::runtime`. Transport/auth — Home launch token,
//! capability tokens, `provider_call`, capsule-local storage — is one
//! implementation shared with the other capsules.
//!
//! # Auth is ElastOS Home. Keys are the capsule.
//!
//! * Home launches us with `#home_token=…` (hash; query still accepted).
//!   We keep it in memory (opaque iframes cannot use sessionStorage) and send
//!   `x-elastos-home-token` on provider calls. There is no Hyper login and no
//!   `Authorization: Bearer` mint.
//! * The token is scrubbed from the visible URL.
//! * Ed25519 + X25519 + ML-KEM live in the capsule (`ensure_local_identity`).
//!   ElastOS `did-provider` is the device DID, not the Hyper social identity.
//!
//! # Two contract facts
//!
//! * **`GET /api/session` returns session METADATA, not identity.** Never treat
//!   `principal` (`person:local:…`) as a social DID.
//! * **A 403/404 on `/api/provider/<scheme>/<op>` is the gateway allowlist**
//!   unless patch 0003 is applied. WASM cannot work around that.

#![allow(unused_imports)]

pub use hey_core::runtime::{
    acquire_boot_capabilities, adopt_device_link, api_base, api_url, content,
    device_connect_payload, ensure_capability_token, home_launch_token, inherit_session, peer,
    provider_call, redeem_launch_token, scrub_launch_token_from_url, session_current, storage,
    DeviceConnect, RuntimeError,
};

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::json;

/// What the shell shows about who we are and whether the runtime is answering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub state: &'static str,
    /// The social DID minted in this capsule after Home authenticated us.
    pub did: String,
    pub name: String,
}

impl Session {
    pub fn booting() -> Self {
        Session { state: "connecting...", did: String::new(), name: String::new() }
    }
}

fn publish_session(set: WriteSignal<Session>, redeemed: bool, did: String) {
    let name = hey_core::session::current()
        .map(|s| s.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_default();
    set.set(Session {
        state: if !did.is_empty() {
            "signed in"
        } else if redeemed {
            "runtime session"
        } else {
            "no runtime"
        },
        did,
        name,
    });
}

/// The boot sequence, in the order Home requires.
///
/// Redeem the launch token before scrubbing it from the URL. Mint capsule
/// keys only after Home has authenticated this load.
///
/// Do not wait for capability grants or did-provider nickname before
/// painting the session. Home's Grant prompt can sit pending for 30s per
/// resource; blocking here left the rail on "connecting..." for minutes.
pub fn boot(set: WriteSignal<Session>) {
    spawn_local(async move {
        let redeemed = redeem_launch_token().await;
        scrub_launch_token_from_url();
        let did = hey_core::api::dms::ensure_local_identity().unwrap_or_default();
        publish_session(set, redeemed, did.clone());

        spawn_local(async move {
            acquire_boot_capabilities().await;
            if let Some(nick) = elastos_nickname().await {
                if let Some(mut s) = hey_core::session::current() {
                    if !nick.is_empty() {
                        s.name = nick;
                        hey_core::session::set(&s);
                    }
                }
            }
            let did = hey_core::session::current()
                .map(|s| s.did_key)
                .filter(|d| !d.is_empty())
                .unwrap_or(did);
            publish_session(set, redeemed, did);
        });
    });

    spawn_local(async {
        hey_social::peer_receiver::register();
        hey_core::peer_receiver::run().await;
    });
}

async fn elastos_nickname() -> Option<String> {
    let v = provider_call("did", "get_nickname", json!({})).await.ok()?;
    let d = v.get("data").unwrap_or(&v);
    d.get("nickname")
        .or_else(|| d.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
