//! Embed the Runtime Wallet app — do not reimplement it.
//!
//! Home launches first-party apps at `{api_base}/apps/{name}/?home_token=…`
//! (same pattern as chat-room, documents, library, system). This tab iframes
//! that mount. Same origin, so the browser sends existing Runtime cookies.
//! If hey-core still holds this capsule's Home launch token, it is appended
//! so Wallet can redeem the same way Home would. We never mint a token.

use hey_core::runtime::{api_base, home_launch_token};
use leptos::prelude::*;

fn wallet_src() -> String {
    let mut url = format!("{}/apps/wallet/", api_base());
    if let Some(tok) = home_launch_token() {
        let enc = String::from(js_sys::encode_uri_component(&tok));
        url.push_str("?home_token=");
        url.push_str(&enc);
    }
    url
}

#[component]
pub fn Wallet() -> impl IntoView {
    let src = wallet_src();
    view! {
        <section class="plane wallet-embed">
            <header class="bar">
                <h1>"Wallet"</h1>
            </header>
            <p class="note wallet-note">
                "Money is ElastOS Wallet, not a Hyper mnemonic. This tab opens the Runtime Wallet app. Chat keys in this capsule cannot spend."
            </p>
            <iframe
                class="wallet-frame"
                src=src
                title="ElastOS Wallet"
            ></iframe>
        </section>
    }
}
