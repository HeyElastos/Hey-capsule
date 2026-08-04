//! The only file that knows the runtime's wire shape.
//!
//! A thin shim over `hey_core::runtime`, the same way hey-social's is. The
//! transport/auth core — launch-token redemption, capability tokens,
//! `provider_call`, storage dispatch, session introspection — is ONE
//! implementation shared with the other capsules and with the phone and desktop
//! apps. Re-exported here so the rest of this capsule has a single import to
//! reach for and the contract has a single place to change.
//!
//! # This capsule does not do identity, and must not
//!
//! There is no login here, no key, no DID of our own. ElastOS owns all three:
//!
//! * The runtime launches us with `?home_token=…` on the URL. We redeem it and
//!   the runtime sets an **HttpOnly, app-scoped session cookie**. We never see
//!   the cookie; `credentials: include` carries it. There is no
//!   `Authorization: Bearer` header anywhere in this capsule, and adding one
//!   would be the wrong shape.
//! * The token is then scrubbed from the visible URL, so it cannot leak through
//!   a screenshot, a bookmark or browser history.
//! * The signing key stays with the runtime. When the DM engine needs a
//!   signature or a decapsulation it asks the identity provider — the capsule
//!   asks for the operation, never for the key. `adopt_provider_identity`
//!   installs a session whose local seed is deliberately EMPTY.
//!
//! Any legacy session that still carries a local seed is dropped at boot rather
//! than migrated. A seed sitting in localStorage is an XSS surface, and the
//! whole point of the runtime-held-key model is that it should not be there.
//!
//! # Two contract facts that look like bugs until you check them
//!
//! * **`GET /api/session` returns session METADATA, not identity.** No `did`,
//!   no `principal`, no `identity.did` on stock upstream. `inherit_session`
//!   probes DID-shaped fields and deliberately skips `principal`, because a
//!   runtime principal (`person:local:…`) is NOT a social DID and treating it
//!   as one re-creates a bug the messaging audit already found once.
//! * **A 403 on `/api/provider/<scheme>/<op>` is the gateway's provider-proxy
//!   allowlist**, not a bad capability token. It is hardcoded to a couple of app
//!   names upstream. There is no client-side way around it — the runtime is the
//!   gatekeeper, and anyone proposing an in-capsule workaround for a 401→403
//!   pattern is solving the wrong problem.
//!
//! Source for both: `docs/runtime-quick-reference.md`, audited against upstream
//! `6d4c385`.

#![allow(unused_imports)]

pub use hey_core::runtime::{
    acquire_boot_capabilities, api_base, api_url, content, ensure_capability_token,
    home_launch_token, inherit_session, peer, provider_call, redeem_launch_token,
    scrub_launch_token_from_url, session_current, storage, RuntimeError,
};

use leptos::prelude::*;
use leptos::task::spawn_local;

/// What the shell shows about who we are and whether the runtime is answering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub state: &'static str,
    /// The social DID, once the runtime has given us one. Never invented here.
    pub did: String,
    pub name: String,
}

impl Session {
    pub fn booting() -> Self {
        Session { state: "connecting\u{2026}", did: String::new(), name: String::new() }
    }
}

/// The boot sequence, in the order the runtime requires.
///
/// Narrow on purpose, and the order matters: redeem before scrubbing (the token
/// has to be read before it is removed), and scrub before anything slow, so a
/// token cannot sit in the address bar while the network is waited on.
pub fn boot(set: WriteSignal<Session>) {
    // Drop any legacy session that still carries a local seed. Identity is
    // re-derived from the runtime below; a seed here would be a leftover from
    // the pre-runtime model and an XSS surface for no benefit.
    if let Some(s) = hey_core::session::current() {
        if !s.auth_key_hex.is_empty() {
            hey_core::session::clear();
        }
    }

    spawn_local(async move {
        let redeemed = redeem_launch_token().await;
        scrub_launch_token_from_url();
        acquire_boot_capabilities().await;

        // Ask the runtime who we are. Two probes, in this order: the session
        // payload if this runtime exposes a DID-shaped field, then the identity
        // provider, which is the model that actually holds the key.
        let from_session = inherit_session().await;
        let did = match hey_core::api::dms::adopt_provider_identity().await {
            Some(d) => d,
            None => from_session.as_ref().map(|s| s.did_key.clone()).unwrap_or_default(),
        };

        let name = hey_core::session::current()
            .map(|s| s.name)
            .filter(|n| !n.is_empty())
            .unwrap_or_default();

        set.set(Session {
            state: if !did.is_empty() {
                "signed in"
            } else if redeemed {
                // Redeemed but no identity surface: expected on stock upstream,
                // where /api/session carries no DID and the identity provider
                // is not in the proxy allowlist. Not an error to shout about.
                "runtime session"
            } else {
                "no runtime"
            },
            did,
            name,
        });
    });
}
