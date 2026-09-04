//! Start a relationship on ElastOS Runtime: chat invite and follow link.
//!
//! The spine was drawing empty lists because nothing here could mint or
//! accept a link. Transport stays `elastos://peer/*` and `elastos://content/*`.
//! Opaque Home iframes block `navigator.clipboard`, so copy uses a textarea.

use hey_core::api::dms::{self, IdentityMode};
use hey_social::api::profile;
use leptos::callback::Callback;
use leptos::prelude::*;
use leptos::task::spawn_local;

pub fn copy_text(text: &str) -> bool {
    let Some(win) = web_sys::window() else {
        return false;
    };
    // Opaque Home iframes often refuse clipboard. The invite textarea stays
    // selectable. This is best-effort.
    let _ = win.navigator().clipboard().write_text(text);
    true
}

/// Shown when this load was not launched from the Home dock.
#[component]
pub fn HomeGate(session: ReadSignal<crate::runtime::Session>) -> impl IntoView {
    view! {
        <Show when=move || session.get().state == "no runtime" fallback=|| ().into_view()>
            <div class="home-gate">
                <div class="card">
                    <h2>"Launch Hyper from Home"</h2>
                    <p>
                        "ElastOS Home authenticates this capsule. Open Hyper from the dock so Home can pass a launch token. A direct URL has no session, so Messages and Social stay empty."
                    </p>
                </div>
            </div>
        </Show>
    }
}

/// Mint or accept a sealed chat invite (`hey-invite:`).
#[component]
pub fn InviteSheet(
    open: RwSignal<bool>,
    #[prop(into)] on_joined: Callback<String>,
) -> impl IntoView {
    let tab = RwSignal::new("create");
    let link = RwSignal::new(String::new());
    let paste = RwSignal::new(String::new());
    let err = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let copied = RwSignal::new(false);

    let generate = move || {
        if busy.get() {
            return;
        }
        err.set(String::new());
        busy.set(true);
        spawn_local(async move {
            match dms::generate_invite("", IdentityMode::Regular, "").await {
                Ok(l) => link.set(l),
                Err(e) => err.set(e),
            }
            busy.set(false);
        });
    };

    let accept = move || {
        if busy.get() {
            return;
        }
        let token = dms::qr_unwrap_scan(paste.get().trim());
        if token.is_empty() {
            return;
        }
        err.set(String::new());
        busy.set(true);
        spawn_local(async move {
            match dms::accept_invite(&token, IdentityMode::Regular).await {
                Ok(did) => {
                    paste.set(String::new());
                    open.set(false);
                    on_joined.run(did);
                }
                Err(e) => err.set(e),
            }
            busy.set(false);
        });
    };

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div class="sheet-wrap" on:click=move |_| open.set(false)>
                <div class="sheet" on:click=move |e| e.stop_propagation()>
                    <header class="bar">
                        <h1>"New chat"</h1>
                        <div class="spring"></div>
                        <button class="btn ghost" on:click=move |_| open.set(false)>"Close"</button>
                    </header>
                    <div class="sheet-scroll">
                        <div class="btn-row">
                            <button
                                class="btn"
                                class:primary=move || tab.get() == "create"
                                on:click=move |_| tab.set("create")
                            >
                                "Create invite"
                            </button>
                            <button
                                class="btn"
                                class:primary=move || tab.get() == "accept"
                                on:click=move |_| tab.set("accept")
                            >
                                "Accept invite"
                            </button>
                        </div>
                        <Show when=move || tab.get() == "create" fallback=move || view! {
                            <p class="note">"Paste a hey-invite link, a HEYI QR payload, or the token your contact shared."</p>
                            <textarea
                                class="field invite-paste"
                                placeholder="hey-invite:…"
                                prop:value=move || paste.get()
                                on:input=move |e| paste.set(event_target_value(&e))
                            ></textarea>
                            <button class="btn primary" disabled=move || busy.get() on:click=move |_| accept()>
                                {move || if busy.get() { "Joining…" } else { "Join chat" }}
                            </button>
                        }>
                            <p class="note">"Share this with one person. Carrier carries the handshake. Keys stay in this capsule."</p>
                            <button class="btn primary" disabled=move || busy.get() on:click=move |_| generate()>
                                {move || if busy.get() { "Minting…" } else { "Mint invite" }}
                            </button>
                            <Show when=move || !link.get().is_empty() fallback=|| ().into_view()>
                                {move || {
                                    let l = link.get();
                                    let qr = dms::invite_qr_svg(&dms::qr_wrap_link(&l)).unwrap_or_default();
                                    view! {
                                        <div class="qr-wrap" inner_html=qr></div>
                                        <textarea class="field invite-paste" readonly prop:value=l.clone()></textarea>
                                        <button class="btn" on:click=move |_| {
                                            copied.set(copy_text(&l));
                                        }>
                                            {move || if copied.get() { "Copied" } else { "Copy link" }}
                                        </button>
                                    }
                                }}
                            </Show>
                        </Show>
                        <Show when=move || !err.get().is_empty() fallback=|| ().into_view()>
                            <p class="note">{move || err.get()}</p>
                        </Show>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Follow from a hey-friend link or a bare did:key.
#[component]
pub fn FollowSheet(open: RwSignal<bool>) -> impl IntoView {
    let paste = RwSignal::new(String::new());
    let mine = RwSignal::new(String::new());
    let err = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let copied = RwSignal::new(false);

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div class="sheet-wrap" on:click=move |_| open.set(false)>
                <div class="sheet" on:click=move |e| e.stop_propagation()>
                    <header class="bar">
                        <h1>"Follow"</h1>
                        <div class="spring"></div>
                        <button class="btn ghost" on:click=move |_| open.set(false)>"Close"</button>
                    </header>
                    <div class="sheet-scroll">
                        <p class="note">"Paste their hey-friend link or did:key. Posts travel on ElastOS content. Chat still needs a hey-invite unless the friend link carries DM keys."</p>
                        <textarea
                            class="field invite-paste"
                            placeholder="hey-friend:… or did:key:z…"
                            prop:value=move || paste.get()
                            on:input=move |e| paste.set(event_target_value(&e))
                        ></textarea>
                        <button class="btn primary" disabled=move || busy.get() on:click=move |_| {
                            if busy.get() { return; }
                            let token = paste.get().trim().to_string();
                            if token.is_empty() { return; }
                            err.set(String::new());
                            busy.set(true);
                            spawn_local(async move {
                                match profile::follow_link(&token).await {
                                    Ok(_) => {
                                        paste.set(String::new());
                                        open.set(false);
                                    }
                                    Err(e) => err.set(e.to_string()),
                                }
                                busy.set(false);
                            });
                        }>
                            {move || if busy.get() { "Following…" } else { "Follow" }}
                        </button>
                        <button class="btn ghost" on:click=move |_| {
                            spawn_local(async move {
                                match profile::my_friend_link().await {
                                    Ok(l) => mine.set(l),
                                    Err(e) => err.set(e.to_string()),
                                }
                            });
                        }>"Show my follow link"</button>
                        <Show when=move || !mine.get().is_empty() fallback=|| ().into_view()>
                            {move || {
                                let l = mine.get();
                                view! {
                                    <textarea class="field invite-paste" readonly prop:value=l.clone()></textarea>
                                    <button class="btn" on:click=move |_| {
                                        copied.set(copy_text(&l));
                                    }>
                                        {move || if copied.get() { "Copied" } else { "Copy follow link" }}
                                    </button>
                                }
                            }}
                        </Show>
                        <Show when=move || !err.get().is_empty() fallback=|| ().into_view()>
                            <p class="note">{move || err.get()}</p>
                        </Show>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Mint a short-lived Home session code for another screen, or paste one.
/// Same job as Skia's "Link a device": this machine already has Home, the
/// other screen borrows that launch. Codes expire in a few minutes.
#[component]
pub fn LinkDeviceSheet(open: RwSignal<bool>) -> impl IntoView {
    let tab = RwSignal::new("show");
    let tick = RwSignal::new(0u32);
    let paste = RwSignal::new(String::new());
    let err = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let copied = RwSignal::new(false);

    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(60_000).await;
                if !open.get_untracked() {
                    break;
                }
                tick.update(|t| *t += 1);
            }
        });
    });

    let accept = move || {
        if busy.get() {
            return;
        }
        let raw = paste.get().trim().to_string();
        if raw.is_empty() {
            return;
        }
        err.set(String::new());
        busy.set(true);
        spawn_local(async move {
            match crate::runtime::adopt_device_link(&raw).await {
                Ok(()) => {
                    paste.set(String::new());
                    open.set(false);
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().reload();
                    }
                }
                Err(e) => err.set(e),
            }
            busy.set(false);
        });
    };

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div class="sheet-wrap" on:click=move |_| open.set(false)>
                <div class="sheet" on:click=move |e| e.stop_propagation()>
                    <header class="bar">
                        <h1>"Link a device"</h1>
                        <div class="spring"></div>
                        <button class="btn ghost" on:click=move |_| open.set(false)>"Close"</button>
                    </header>
                    <div class="sheet-scroll">
                        <div class="btn-row">
                            <button
                                class="btn"
                                class:primary=move || tab.get() == "show"
                                on:click=move |_| tab.set("show")
                            >
                                "Show code"
                            </button>
                            <button
                                class="btn"
                                class:primary=move || tab.get() == "paste"
                                on:click=move |_| tab.set("paste")
                            >
                                "Paste a code"
                            </button>
                        </div>
                        <Show when=move || tab.get() == "show" fallback=move || view! {
                            <p class="note">
                                "Paste the URL, heyapp link, or dl1 code another Hyper screen showed. This capsule then borrows that Home session."
                            </p>
                            <textarea
                                class="field invite-paste"
                                placeholder="https://…/#home_token=dl1.… or heyapp://connect?…"
                                prop:value=move || paste.get()
                                on:input=move |e| paste.set(event_target_value(&e))
                            ></textarea>
                            <button class="btn primary" disabled=move || busy.get() on:click=move |_| accept()>
                                {move || if busy.get() { "Linking…" } else { "Use this code" }}
                            </button>
                        }>
                            {move || {
                                tick.get();
                                match crate::runtime::device_connect_payload("hyper-desktop") {
                                    Some(p) => {
                                        let qr = dms::invite_qr_svg(&p.page_url).unwrap_or_default();
                                        let page = p.page_url.clone();
                                        let code = p.code.clone();
                                        view! {
                                            <p class="note">
                                                "This machine will hold chats and files for every screen that opens the code. Scan the QR, or type the dl1 code on the other device. It refreshes every minute and expires shortly after."
                                            </p>
                                            <div class="qr-wrap" inner_html=qr></div>
                                            <textarea class="field invite-paste" readonly prop:value=page.clone()></textarea>
                                            <button class="btn" on:click=move |_| {
                                                copied.set(copy_text(&page));
                                            }>
                                                {move || if copied.get() { "Copied URL" } else { "Copy URL" }}
                                            </button>
                                            <p class="note">"Link code"</p>
                                            <textarea class="field invite-paste" readonly prop:value=code.clone()></textarea>
                                            <button class="btn" on:click=move |_| {
                                                copied.set(copy_text(&code));
                                            }>
                                                {move || if copied.get() { "Copied code" } else { "Copy code" }}
                                            </button>
                                        }.into_any()
                                    }
                                    None => view! {
                                        <p class="note">
                                            "Launch Hyper from the Home dock first. A direct URL has no session to share."
                                        </p>
                                    }.into_any(),
                                }
                            }}
                        </Show>
                        <Show when=move || !err.get().is_empty() fallback=|| ().into_view()>
                            <p class="note">{move || err.get()}</p>
                        </Show>
                    </div>
                </div>
            </div>
        </Show>
    }
}
