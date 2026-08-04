//! The only file that knows the runtime's wire shape.
//!
//! Everything else calls in here. When the runtime contract moves, this is the
//! file that changes and nothing else should have to.
//!
//! Three things the contract says that look wrong until you check them:
//!
//! * **`GET /api/session` returns session METADATA, not identity.** The body is
//!   `{session_id, session_type, vm_id, capabilities_count, created_at,
//!   last_active}`. There is no `did`, no `principal`, no `identity.did`. Code
//!   that goes looking for one finds `undefined` and invents a bug.
//!
//! * **No `Authorization: Bearer` header, ever.** The runtime sets an HttpOnly
//!   cookie and `credentials: include` carries it. Extracting a token from
//!   launch-token redemption and putting it in a header is the wrong shape.
//!
//! * **A 403 on `/api/provider/<scheme>/<op>` is almost always the gateway's
//!   provider-proxy allowlist, not a bad capability token.** It is hardcoded to
//!   a couple of app names upstream, and there is no client-side way around it
//!   — the runtime is the gatekeeper. Anyone proposing an in-capsule workaround
//!   for a 401→403 pattern is solving the wrong problem.
//!
//! Source for all three: `docs/runtime-quick-reference.md`, audited against
//! upstream `6d4c385`.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestCredentials, RequestInit, Response};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub state: &'static str,
    pub capabilities: u32,
}

impl Session {
    pub fn unknown() -> Self {
        Session { state: "checking\u{2026}", capabilities: 0 }
    }
}

/// Ask the runtime for the session, once.
///
/// Failure is not an error state worth shouting about: a capsule opened outside
/// a runtime (trunk serve, a plain browser) simply has no session, and the
/// shell is still worth showing. So this reports "no runtime" and moves on
/// rather than blocking the UI behind a gate.
pub fn probe_session(set: WriteSignal<Session>) {
    leptos::task::spawn_local(async move {
        match fetch_session().await {
            Some(n) => set.set(Session { state: "active", capabilities: n }),
            None => set.set(Session { state: "no runtime", capabilities: 0 }),
        }
    });
}

async fn fetch_session() -> Option<u32> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    // The cookie is HttpOnly; this is what carries it. Without it the request
    // is unauthenticated and the runtime answers 401.
    opts.set_credentials(RequestCredentials::Include);

    let req = Request::new_with_str_and_init("/api/session", &opts).ok()?;
    let win = web_sys::window()?;
    let resp: Response = JsFuture::from(win.fetch_with_request(&req)).await.ok()?.dyn_into().ok()?;
    if !resp.ok() {
        return None;
    }
    let json = JsFuture::from(resp.json().ok()?).await.ok()?;

    // `capabilities_count`, and only that. Reaching for `did` here is the
    // documented mistake — the field does not exist on stock upstream.
    let n = js_sys::Reflect::get(&json, &"capabilities_count".into()).ok()?;
    n.as_f64().map(|f| f as u32)
}
