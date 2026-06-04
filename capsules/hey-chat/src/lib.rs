use std::borrow::Cow;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::path;
use leptos_router::NavigateOptions;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, Event, File, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent, Url};

use hey_core::api::dms::{
    accept_invite, add_group_members, create_group, delete_conversation, delete_group,
    fetch_attachment, generate_invite, invite_qr_svg, list_contacts, list_groups, mark_group_read,
    mark_read, read_conversation, read_group_conversation, revoke_invite, send_group_message,
    send_group_message_with_attachments, send_message, send_message_with_attachments,
    upload_attachment, Attachment, DmContact, DmMessage, Group, IdentityMode,
};
use hey_core::runtime::device_link_url;
use hey_core::session;

// Derive the router base from the iframe mount path. Under YunoHost the
// capsule loads at e.g. `/apps/hey-chat/` — without this the Router
// sees the full pathname and matches nothing. Same heuristic as hey-social.
fn router_base() -> Cow<'static, str> {
    (|| -> Option<String> {
        let win = web_sys::window()?;
        let path = win.location().pathname().ok()?;
        let idx = path.find("/apps/")?;
        let after = &path[idx + 6..];
        let end = after.find('/').map(|j| idx + 6 + j).unwrap_or(path.len());
        Some(path[..end].to_string())
    })()
    .map(Cow::Owned)
    .unwrap_or(Cow::Borrowed(""))
}

#[component]
pub fn App() -> impl IntoView {
    // Boot against the shared engine (ctx::init already ran in main):
    //   1. redeem any ?home_token=… into an app-scoped session,
    //   2. scrub the token from the visible URL,
    //   3. pre-warm the capability tokens this capsule declared,
    //   4. start the chat receive loop (no-op while signed out).
    spawn_local(async {
        let _ = hey_core::runtime::redeem_launch_token().await;
        hey_core::runtime::scrub_launch_token_from_url();
        hey_core::runtime::acquire_boot_capabilities().await;
    });
    spawn_local(async {
        hey_core::peer_receiver::run().await;
    });

    let base = router_base();
    view! {
        <Router base=base>
            <Routes fallback=|| view! { <p>"Not found"</p> }>
                <Route path=path!("/") view=Root />
                <Route path=path!("/chat/:did") view=Root />
                <Route path=path!("/group/:gid") view=Root />
            </Routes>
        </Router>
    }
}

/// Root view: the runtime sign-in gate wraps the Telegram-desktop shell.
#[component]
fn Root() -> impl IntoView {
    view! { <SignInGate /> }
}

// ── Runtime-only auth gate ────────────────────────────────────────────────
//
// Identity comes ONLY from the Elastos runtime — either the identity provider
// (`identity/whoami`: a provider-backed did:key with NO local seed, the runtime
// signs & decrypts) or an inherited runtime session (`/api/session`, wallet SSO
// from Home's launch token). There is deliberately NO local-seed / passkey
// fallback: without the runtime there is no signing key in the browser, so the
// app stays gated — no runtime, no app. A seed therefore never lives in
// localStorage (the XSS-exfiltration surface the old passkey path carried).
#[derive(Clone, Copy, PartialEq)]
enum Gate {
    Checking,
    Ready,
    Offline,
}

/// Ask the runtime who we are: provider identity first (no local seed), then an
/// inherited runtime session. Returns true once a session is in place.
async fn probe_runtime() -> bool {
    if hey_core::api::dms::adopt_provider_identity().await.is_some() {
        return true;
    }
    if let Some(inherited) = hey_core::runtime::inherit_session().await {
        session::set(&inherited);
        return true;
    }
    false
}

#[component]
fn SignInGate() -> impl IntoView {
    // A legacy session carrying a local Ed25519 seed (auth_key_hex set) predates
    // this gate. Drop it so no seed lingers in localStorage; identity is
    // re-derived from the runtime below. clear() only removes the localStorage
    // session record — it leaves the launch-token / capability caches in
    // sessionStorage intact so the runtime probe can still redeem.
    if let Some(s) = session::current() {
        if !s.auth_key_hex.is_empty() {
            session::clear();
        }
    }

    let gate = RwSignal::new(match session::current() {
        // A runtime-backed session (did:key, empty seed) means we're already in.
        Some(s) if !s.did_key.is_empty() => Gate::Ready,
        _ => Gate::Checking,
    });

    // First probe on mount (skip if a runtime session already persisted).
    Effect::new(move |_| {
        if gate.get_untracked() != Gate::Checking {
            return;
        }
        spawn_local(async move {
            gate.set(if probe_runtime().await { Gate::Ready } else { Gate::Offline });
        });
    });

    view! {
        <Show
            when=move || gate.get() == Gate::Ready
            fallback=move || view! { <RuntimeGate gate=gate /> }
        >
            <Shell />
        </Show>
    }
}

// ── RuntimeGate: shown until the runtime projects an identity in ───────────
//
// Replaces the old in-capsule passkey card. The runtime (Home) authenticates
// the user; this capsule only adopts the projected identity. While we ask, show
// a connecting state; if the runtime is unreachable, block with a clear message
// + retry. The app does not open without the runtime.
#[component]
fn RuntimeGate(gate: RwSignal<Gate>) -> impl IntoView {
    let retry = move |_| {
        if gate.get() == Gate::Checking {
            return;
        }
        gate.set(Gate::Checking);
        spawn_local(async move {
            gate.set(if probe_runtime().await { Gate::Ready } else { Gate::Offline });
        });
    };

    view! {
        <div class="msgr-signin">
            // Gently-drifting colorful abstract symbols behind the card.
            <svg class="msgr-sym msgr-drift-a msgr-sym-warm" style="top:14%; left:12%; width:78px; height:78px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/></svg>
            <svg class="msgr-sym msgr-drift-b msgr-sym-sky" style="top:24%; left:22%; width:60px; height:60px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 21 20H3z"/></svg>
            <svg class="msgr-sym msgr-drift-c msgr-sym-rose" style="bottom:22%; left:14%; width:52px; height:52px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>
            <svg class="msgr-sym msgr-drift-d msgr-sym-orange" style="top:28%; right:16%; width:58px; height:58px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.5 5.5l2.8 2.8M15.7 15.7l2.8 2.8M5.5 18.5l2.8-2.8M15.7 8.3l2.8-2.8"/></svg>
            <svg class="msgr-sym msgr-drift-a msgr-sym-emerald" style="bottom:24%; right:14%; width:70px; height:70px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="3"/></svg>
            <svg class="msgr-sym msgr-drift-b msgr-sym-violet" style="top:58%; right:24%; width:88px; height:88px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><circle cx="12" cy="12" r="7"/><circle cx="12" cy="12" r="11"/></svg>
            <svg class="msgr-sym msgr-drift-c msgr-sym-lime" style="bottom:16%; left:54%; width:50px; height:50px;" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2 14.6 9.3 22 10l-5.8 4.9L18 22l-6-4-6 4 1.8-7.1L2 10l7.4-.7z"/></svg>
            <div class="msgr-signin-card">
                <div class="msgr-signin-logo">"💬"</div>
                <h1 class="msgr-signin-title">"Hey Chat"</h1>
                <p class="msgr-signin-sub">
                    {move || if gate.get() == Gate::Checking {
                        "Connecting to your Elastos runtime…"
                    } else {
                        "Hey Chat gets your identity from your Elastos runtime. The runtime isn't reachable right now — open Hey from your runtime's Home, then retry."
                    }}
                </p>
                {move || if gate.get() == Gate::Offline {
                    let retry = retry.clone();
                    view! {
                        <button
                            type="button"
                            class="msgr-btn-primary msgr-signin-btn"
                            on:click=retry
                        >
                            "Retry"
                        </button>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>
        </div>
    }
}

// ── Shell: 2-pane Telegram-desktop layout ────────────────────────────────
#[component]
fn Shell() -> impl IntoView {
    let params = use_params_map();
    let active_did =
        move || params.read().get("did").map(|s| s.to_string()).unwrap_or_default();
    let active_gid =
        move || params.read().get("gid").map(|s| s.to_string()).unwrap_or_default();

    view! {
        <div class="msgr-shell">
            <aside class="msgr-sidebar">
                <ChatList
                    active_did=Signal::derive(active_did)
                    active_gid=Signal::derive(active_gid)
                />
            </aside>
            <section class="msgr-main">
                {move || {
                    let gid = active_gid();
                    let did = active_did();
                    if !gid.is_empty() {
                        view! { <GroupConversation gid=gid /> }.into_any()
                    } else if !did.is_empty() {
                        view! { <Conversation did=did /> }.into_any()
                    } else {
                        view! { <EmptyState /> }.into_any()
                    }
                }}
            </section>
        </div>
    }
}

#[component]
fn EmptyState() -> impl IntoView {
    view! {
        <div class="msgr-empty">
            <div class="msgr-empty-icon">"💬"</div>
            <h2 class="msgr-empty-title">"Select a chat"</h2>
            <p class="msgr-empty-sub">"Pick a conversation on the left, or add a contact to start."</p>
        </div>
    }
}

// ── ChatList ─────────────────────────────────────────────────────────────
#[component]
fn ChatList(active_did: Signal<String>, active_gid: Signal<String>) -> impl IntoView {
    let contacts: RwSignal<Vec<DmContact>> = RwSignal::new(Vec::new());
    let groups: RwSignal<Vec<Group>> = RwSignal::new(Vec::new());
    let add_open = RwSignal::new(false);
    let link_open = RwSignal::new(false);
    let net_open = RwSignal::new(false);
    let group_open = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    // Carrier connectivity for the status pill — so users can SEE whether the
    // P2P transport is online / connecting / offline (it can wedge), plus how
    // many sends are still queued.
    let health: RwSignal<hey_core::runtime::peer::CarrierHealth> = RwSignal::new(Default::default());
    let queued: RwSignal<usize> = RwSignal::new(0);
    let navigate = use_navigate();

    // Load + refresh the contact list every ~3s so messages arriving via
    // the peer_receiver surface without a manual refresh.
    Effect::new(move |_| {
        spawn_local(async move {
            loop {
                contacts.set(list_contacts().await);
                groups.set(list_groups().await);
                wait_ms(3000).await;
            }
        });
    });

    // Probe carrier health + outbox backlog every ~5s for the status pill.
    Effect::new(move |_| {
        spawn_local(async move {
            loop {
                health.set(hey_core::runtime::peer::carrier_health().await);
                queued.set(hey_core::api::outbox::pending_count().await);
                wait_ms(5000).await;
            }
        });
    });

    view! {
        <div class="msgr-list">
            <header class="msgr-list-header">
                <h1 class="msgr-list-title">"Hey Chat"</h1>
                <button
                    type="button"
                    class="msgr-add-btn"
                    title="Link phone"
                    aria-label="Link phone"
                    on:click=move |_| link_open.set(true)
                >
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="3" width="7" height="7" rx="1" />
                        <rect x="14" y="3" width="7" height="7" rx="1" />
                        <rect x="3" y="14" width="7" height="7" rx="1" />
                        <path d="M14 14h3v3M21 14v3M14 18v3h3M18 21h3" />
                    </svg>
                </button>
                <button
                    type="button"
                    class="msgr-add-btn"
                    title="New group"
                    aria-label="New group"
                    on:click=move |_| group_open.set(true)
                >
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
                        <circle cx="9" cy="7" r="4" />
                        <path d="M23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75" />
                    </svg>
                </button>
                <button
                    type="button"
                    class="msgr-add-btn"
                    title="Add contact"
                    aria-label="Add contact"
                    on:click=move |_| add_open.set(true)
                >
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M12 5v14M5 12h14" />
                    </svg>
                </button>
                <button
                    type="button"
                    class="msgr-add-btn"
                    title="Network / P2P settings"
                    aria-label="Network / P2P settings"
                    on:click=move |_| net_open.set(true)
                >
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="3" />
                        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                    </svg>
                </button>
            </header>

            <div class="msgr-search" style="padding:2px 8px 8px;">
                <input
                    type="text"
                    placeholder="Search chats…"
                    prop:value=move || search.get()
                    on:input=move |ev| search.set(event_target_value(&ev))
                    style="width:100%;box-sizing:border-box;padding:7px 12px;border-radius:9px;\
                           border:1px solid rgba(127,127,127,0.22);background:rgba(127,127,127,0.08);\
                           color:inherit;font-size:14px;outline:none;"
                />
            </div>

            <div class="msgr-list-rows">
                {move || {
                    let q = search.get();
                    if groups.get().is_empty() && contacts.get().is_empty() {
                        view! { <div class="msgr-list-empty"><p>"No chats yet — add a contact or start a group."</p></div> }.into_any()
                    } else if !q.is_empty()
                        && filtered_groups(&groups.get(), &q).is_empty()
                        && filtered_contacts(&contacts.get(), &q).is_empty()
                    {
                        view! { <div class="msgr-list-empty"><p>"No matches."</p></div> }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
                <For
                    each=move || filtered_groups(&groups.get(), &search.get())
                    key=|g| format!("{}:{}:{}", g.id, g.last_ts, g.unread)
                    children={
                        let navigate = navigate.clone();
                        move |g: Group| {
                            let navigate = navigate.clone();
                            let gid = g.id.clone();
                            let is_active = active_gid.get() == g.id;
                            let row_class = if is_active { "msgr-row msgr-row-active" } else { "msgr-row" };
                            let preview = if g.last_preview.is_empty() {
                                format!("{} members", g.members.len())
                            } else {
                                g.last_preview.clone()
                            };
                            let unread = g.unread;
                            let del_gid = g.id.clone();
                            view! {
                                <button
                                    type="button"
                                    class=row_class
                                    on:click=move |_| navigate(&format!("/group/{}", gid), NavigateOptions::default())
                                >
                                    <div class="msgr-avatar" style="display:flex;align-items:center;justify-content:center;font-size:18px;background:linear-gradient(135deg,#6d6ef5,#a06df5);color:#fff;">"👥"</div>
                                    <div class="msgr-row-body">
                                        <div class="msgr-row-top">
                                            <span class="msgr-row-name">{g.name.clone()}</span>
                                            <span class="msgr-row-time">{ts_short(g.last_ts)}</span>
                                        </div>
                                        <div class="msgr-row-bottom">
                                            <span class="msgr-row-preview">{preview}</span>
                                            {if unread > 0 {
                                                view! { <span class="msgr-badge">{if unread > 99 { "99+".to_string() } else { unread.to_string() }}</span> }.into_any()
                                            } else { ().into_any() }}
                                        </div>
                                    </div>
                                    <span
                                        role="button"
                                        tabindex="0"
                                        class="msgr-row-cancel"
                                        title="Delete group (for you)"
                                        aria-label="Delete group"
                                        on:click=move |ev: leptos::ev::MouseEvent| {
                                            ev.stop_propagation();
                                            let ok = web_sys::window()
                                                .and_then(|w| w.confirm_with_message("Remove this group from this device? Other members keep it.").ok())
                                                .unwrap_or(false);
                                            if !ok { return; }
                                            let d2 = del_gid.clone();
                                            groups.update(|l| l.retain(|x| x.id != d2));
                                            let d3 = del_gid.clone();
                                            spawn_local(async move { let _ = delete_group(&d3).await; });
                                        }
                                    >
                                        "✕"
                                    </span>
                                </button>
                            }
                        }
                    }
                />
                            <For
                                each=move || filtered_contacts(&contacts.get(), &search.get())
                                key=|c| c.did.clone()
                                children={
                                    let navigate = navigate.clone();
                                    move |c: DmContact| {
                                        let navigate = navigate.clone();
                                        let did = c.did.clone();
                                        let pend_did = c.did.clone();
                                        let del_did = c.did.clone();
                                        let is_pending = c.did.starts_with("pending:");
                                        let is_active = active_did.get() == c.did;
                                        let row_class = if is_active {
                                            "msgr-row msgr-row-active"
                                        } else {
                                            "msgr-row"
                                        };
                                        let name = display_name(&c);
                                        let preview = if c.last_preview.is_empty() {
                                            "No messages yet".to_string()
                                        } else {
                                            c.last_preview.clone()
                                        };
                                        let unread = c.unread;
                                        view! {
                                            <button
                                                type="button"
                                                class=row_class
                                                on:click=move |_| {
                                                    navigate(
                                                        &format!("/chat/{}", did),
                                                        NavigateOptions::default(),
                                                    );
                                                }
                                            >
                                                <Avatar name=name.clone() />
                                                <div class="msgr-row-body">
                                                    <div class="msgr-row-top">
                                                        <span class="msgr-row-name">{name.clone()}</span>
                                                        <span class="msgr-row-time">{ts_short(c.last_ts)}</span>
                                                    </div>
                                                    <div class="msgr-row-bottom">
                                                        <span class="msgr-row-preview">{preview}</span>
                                                        {if unread > 0 {
                                                            view! {
                                                                <span class="msgr-badge">
                                                                    {if unread > 99 { "99+".to_string() } else { unread.to_string() }}
                                                                </span>
                                                            }.into_any()
                                                        } else {
                                                            ().into_any()
                                                        }}
                                                    </div>
                                                </div>
                                                {if is_pending {
                                                    let d = pend_did.clone();
                                                    view! {
                                                        <span
                                                            role="button"
                                                            tabindex="0"
                                                            class="msgr-row-cancel"
                                                            title="Cancel invite"
                                                            aria-label="Cancel invite"
                                                            on:click=move |ev: leptos::ev::MouseEvent| {
                                                                ev.stop_propagation();
                                                                let d2 = d.clone();
                                                                contacts.update(|l| l.retain(|x| x.did != d2));
                                                                let d3 = d.clone();
                                                                spawn_local(async move { let _ = revoke_invite(&d3).await; });
                                                            }
                                                        >
                                                            "✕"
                                                        </span>
                                                    }.into_any()
                                                } else {
                                                    // Active conversation: delete (for this device) after a
                                                    // confirm. Removes the contact, message log, ratchet,
                                                    // queues, and attachment blobs (delete_conversation).
                                                    let d = del_did.clone();
                                                    view! {
                                                        <span
                                                            role="button"
                                                            tabindex="0"
                                                            class="msgr-row-cancel"
                                                            title="Delete conversation"
                                                            aria-label="Delete conversation"
                                                            on:click=move |ev: leptos::ev::MouseEvent| {
                                                                ev.stop_propagation();
                                                                let confirmed = web_sys::window()
                                                                    .and_then(|w| {
                                                                        w.confirm_with_message(
                                                                            "Delete this conversation? It is removed for you only — messages, files, and keys for this contact are erased on this device.",
                                                                        )
                                                                        .ok()
                                                                    })
                                                                    .unwrap_or(false);
                                                                if !confirmed {
                                                                    return;
                                                                }
                                                                let d2 = d.clone();
                                                                contacts.update(|l| l.retain(|x| x.did != d2));
                                                                let d3 = d.clone();
                                                                spawn_local(async move { let _ = delete_conversation(&d3).await; });
                                                            }
                                                        >
                                                            "✕"
                                                        </span>
                                                    }.into_any()
                                                }}
                                            </button>
                                        }
                                    }
                                }
                            />
            </div>
            // Carrier status — pinned to the bottom corner of the sidebar so
            // users can always SEE connectivity without it crowding the header.
            {move || {
                let h = health.get();
                let q = queued.get();
                let (dot, label, tip) = if !h.online {
                    (
                        "\u{1f534}",
                        "Offline".to_string(),
                        "Carrier offline — the server may need a restart".to_string(),
                    )
                } else if h.peer_count == 0 {
                    (
                        "\u{1f7e1}",
                        "Connecting\u{2026}".to_string(),
                        "Carrier online, finding peers\u{2026}".to_string(),
                    )
                } else {
                    (
                        "\u{1f7e2}",
                        format!(
                            "Online \u{00b7} {} peer{}",
                            h.peer_count,
                            if h.peer_count == 1 { "" } else { "s" }
                        ),
                        format!("node {}", h.node_id.chars().take(10).collect::<String>()),
                    )
                };
                let queued_txt =
                    if q > 0 { format!(" \u{00b7} {q} queued") } else { String::new() };
                view! {
                    <footer
                        title=tip
                        style="display:flex;align-items:center;gap:5px;padding:6px 12px;\
                               font-size:12px;opacity:0.75;white-space:nowrap;\
                               border-top:1px solid rgba(127,127,127,0.18);"
                    >
                        <span style="font-size:9px;line-height:1;">{dot}</span>
                        <span>{label}{queued_txt}</span>
                    </footer>
                }
            }}
        </div>
        <AddContactModal open=add_open />
        <LinkPhoneModal open=link_open />
        <NetworkSettingsModal open=net_open />
        <NewGroupModal open=group_open contacts=contacts />
    }
}

// ── Attachments (M7) ──────────────────────────────────────────────────────

/// A file the user picked but hasn't sent yet (raw plaintext bytes, held in
/// memory until send encrypts + uploads it).
#[derive(Clone)]
struct PendingAttachment {
    name: String,
    mime: String,
    bytes: Vec<u8>,
}

/// Read a picked `File`'s bytes (async, via Blob::array_buffer through Deref).
async fn read_file_bytes(file: &File) -> Result<Vec<u8>, String> {
    let buf = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "could not read file".to_string())?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

/// Wrap decrypted bytes in a `blob:` object URL for `<img>` / `<a download>`.
fn bytes_to_object_url(bytes: &[u8], mime: &str) -> Result<String, String> {
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr);
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|_| "blob create failed".to_string())?;
    Url::create_object_url_with_blob(&blob).map_err(|_| "object url failed".to_string())
}

/// Render one received/sent attachment: fetch the ciphertext from the content
/// store, decrypt it E2E (the per-file key rode inside the sealed message), and
/// show it inline (images) or as a download chip (everything else). The blob
/// URL is revoked on unmount.
#[component]
fn AttachmentView(att: Attachment) -> impl IntoView {
    // state: 0 loading, 1 ready, 2 error/timeout
    let url = RwSignal::new(Option::<String>::None);
    let state = RwSignal::new(0u8);
    let is_image = att.mime.starts_with("image/");
    let name = att.name.clone();

    // Fetch the (encrypted) blob, decrypt, make an object URL. The byte fetch
    // goes cross-runtime over content/IPFS and can be slow or stall, so guard it
    // with a timeout that flips to a retryable error instead of an endless
    // "loading" — the user always sees the FILENAME and can tap to retry.
    let att_master = att.clone();
    let do_fetch = move || {
        url.set(None);
        state.set(0);
        // Timeout guard: if no bytes after 25s, surface a retry.
        spawn_local(async move {
            wait_ms(25_000).await;
            if url.get_untracked().is_none() {
                state.set(2);
            }
        });
        let att = att_master.clone();
        spawn_local(async move {
            match fetch_attachment(&att).await {
                Ok(bytes) => match bytes_to_object_url(&bytes, &att.mime) {
                    Ok(u) => {
                        url.set(Some(u));
                        state.set(1);
                    }
                    Err(_) => state.set(2),
                },
                Err(_) => state.set(2),
            }
        });
    };

    // Kick off the first fetch.
    {
        let f = do_fetch.clone();
        Effect::new(move |_| {
            f();
        });
    }
    on_cleanup(move || {
        if let Some(u) = url.get_untracked() {
            let _ = Url::revoke_object_url(&u);
        }
    });

    view! {
        <div class="msgr-att">
            {move || {
                match (state.get(), url.get()) {
                    // Ready: inline image, or a one-click download link for files.
                    (1, Some(u)) => {
                        if is_image {
                            view! { <img class="msgr-att-img" src=u alt=name.clone() /> }.into_any()
                        } else {
                            view! {
                                <a class="msgr-att-file" href=u download=name.clone()
                                    title=format!("Download {}", name)>
                                    "📎 "{name.clone()}" ⬇"
                                </a>
                            }
                            .into_any()
                        }
                    }
                    // Error / timeout: keep the name, offer a retry.
                    (2, _) => {
                        let f = do_fetch.clone();
                        view! {
                            <button
                                class="msgr-att-failed"
                                title="The file didn't load (the other side may be offline) — tap to retry"
                                on:click=move |_| f()
                            >
                                "⚠️ "{name.clone()}" — tap to retry"
                            </button>
                        }
                        .into_any()
                    }
                    // Loading: a mini spinning circle + the filename, so the
                    // user sees the transfer is in progress (content/fetch is a
                    // single call with no byte-progress, so the circle is an
                    // honest indeterminate spinner, not a fake percentage).
                    _ => view! {
                        <span
                            class="msgr-att-loading"
                            style="display:inline-flex;align-items:center;gap:6px;"
                        >
                            <svg width="13" height="13" viewBox="0 0 24 24" style="flex:none;">
                                <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor"
                                    stroke-width="3" stroke-opacity="0.25" />
                                <path fill="none" stroke="currentColor" stroke-width="3"
                                    stroke-linecap="round" d="M12 3 a9 9 0 0 1 9 9">
                                    <animateTransform attributeName="transform" attributeType="XML"
                                        type="rotate" from="0 12 12" to="360 12 12" dur="0.8s"
                                        repeatCount="indefinite" />
                                </path>
                            </svg>
                            "📎 "{name.clone()}" — transferring…"
                        </span>
                    }
                    .into_any(),
                }
            }}
        </div>
    }
}

// ── Conversation ─────────────────────────────────────────────────────────
#[component]
fn Conversation(did: String) -> impl IntoView {
    let messages: RwSignal<Vec<DmMessage>> = RwSignal::new(Vec::new());
    let composer = RwSignal::new(String::new());
    let pending: RwSignal<Vec<PendingAttachment>> = RwSignal::new(Vec::new());
    let busy = RwSignal::new(false);
    // Whether this contact has a live Double Ratchet (forward secrecy) vs the
    // single-shot path — surfaced in the header so users see the protection.
    let ratchet = RwSignal::new(false);

    // Load the conversation when the :did param changes + mark read on open.
    {
        let did_load = did.clone();
        Effect::new(move |_| {
            let d = did_load.clone();
            spawn_local(async move {
                let msgs = read_conversation(&d).await;
                messages.set(msgs);
                mark_read(&d).await;
                if let Some(c) = list_contacts().await.into_iter().find(|x| x.did == d) {
                    ratchet.set(c.ratchet_capable);
                }
            });
        });
    }

    // Poll the active conversation for incoming messages every ~3s.
    {
        let did_poll = did.clone();
        Effect::new(move |_| {
            let d = did_poll.clone();
            spawn_local(async move {
                loop {
                    wait_ms(3000).await;
                    let msgs = read_conversation(&d).await;
                    messages.set(msgs);
                }
            });
        });
    }

    let title = short_did(&did);
    let did_send = did.clone();
    let send = {
        let did = did_send.clone();
        move || {
            if busy.get() {
                return;
            }
            let text = composer.get();
            let files = pending.get();
            // Allow send when there's text OR at least one picked file.
            if text.trim().is_empty() && files.is_empty() {
                return;
            }
            let did = did.clone();
            busy.set(true);
            spawn_local(async move {
                // Optimistic: clear input + pending immediately, then refresh
                // from the engine (which appends the sent message).
                composer.set(String::new());
                pending.set(Vec::new());
                if files.is_empty() {
                    let _ = send_message(&did, &text).await;
                } else {
                    // Encrypt + upload each file, then send the refs E2E-sealed.
                    let mut atts = Vec::new();
                    for f in &files {
                        match upload_attachment(&f.name, &f.mime, &f.bytes).await {
                            Ok(a) => atts.push(a),
                            Err(e) => web_sys::console::warn_1(
                                &format!("[hey-chat] attachment upload failed: {e}").into(),
                            ),
                        }
                    }
                    let _ = send_message_with_attachments(&did, &text, atts).await;
                }
                let updated = read_conversation(&did).await;
                messages.set(updated);
                busy.set(false);
            });
        }
    };

    view! {
        <div class="msgr-conv">
            <header class="msgr-conv-header">
                <Avatar name=title.clone() />
                <div class="msgr-conv-title">
                    <span class="msgr-conv-name">{title.clone()}</span>
                    <span
                        class="msgr-conv-status"
                        title=move || {
                            let base = "Hybrid post-quantum end-to-end encryption: \
                                ML-KEM-768 + X25519 key agreement, ChaCha20-Poly1305, \
                                sealed-sender (the relay never sees who's talking). \
                                Keys never leave your runtime.";
                            if ratchet.get() {
                                format!("{base} Double Ratchet gives forward secrecy + post-compromise security.")
                            } else {
                                format!("{base} Single-shot mode (this contact hasn't completed a ratchet handshake yet).")
                            }
                        }
                    >
                        {move || {
                            if ratchet.get() {
                                "\u{1f512} End-to-end encrypted \u{00b7} post-quantum + Double Ratchet"
                            } else {
                                "\u{1f512} End-to-end encrypted \u{00b7} post-quantum"
                            }
                        }}
                    </span>
                </div>
            </header>

            <div class="msgr-conv-body">
                {move || {
                    let list = messages.get();
                    if list.is_empty() {
                        view! {
                            <div class="msgr-conv-empty">
                                <p>"No messages yet. Say hi 👋"</p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <For
                                each=move || messages.get()
                                key=|m| m.id.clone()
                                children=move |m: DmMessage| view! { <Bubble m=m /> }
                            />
                        }.into_any()
                    }
                }}
            </div>

            <Composer composer=composer pending=pending busy=busy send=send.clone() />
        </div>
    }
}

#[component]
fn Bubble(m: DmMessage) -> impl IntoView {
    let row_class = if m.mine { "msgr-bubble-row msgr-bubble-row-mine" } else { "msgr-bubble-row" };
    let bubble_class = if m.mine { "msgr-bubble msgr-bubble-mine" } else { "msgr-bubble" };
    let ts_text = ts_short(m.ts);
    let lock = if m.encrypted { "🔒" } else { "!" };
    let has_text = !m.text.is_empty();
    let text = m.text.clone();
    let attachments = m.attachments.clone();
    // Group messages carry the sender's name (empty for 1-to-1 DMs).
    let sender = m.sender_name.clone();
    let show_sender = !m.mine && !sender.is_empty();
    view! {
        <div class=row_class>
            <div class=bubble_class>
                {show_sender.then(|| view! {
                    <div class="msgr-bubble-sender" style="font-size:12px;font-weight:600;opacity:0.85;margin-bottom:2px;">
                        {sender}
                    </div>
                })}
                {attachments
                    .into_iter()
                    .map(|a| view! { <AttachmentView att=a /> })
                    .collect_view()}
                {has_text.then(|| view! { <p class="msgr-bubble-text">{text}</p> })}
                <span class="msgr-bubble-meta">
                    {ts_text}" "<span class="msgr-bubble-lock">{lock}</span>
                </span>
            </div>
        </div>
    }
}

// ── Composer ─────────────────────────────────────────────────────────────
#[component]
fn Composer(
    composer: RwSignal<String>,
    pending: RwSignal<Vec<PendingAttachment>>,
    busy: RwSignal<bool>,
    send: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let on_input = move |ev: Event| {
        if let Some(t) = ev.target() {
            if let Ok(i) = t.dyn_into::<HtmlTextAreaElement>() {
                composer.set(i.value());
            }
        }
    };
    // Picking files: read each selected file's bytes into `pending` (held in
    // memory; encryption + upload happen on send). Reset the input so the same
    // file can be re-picked.
    let on_file = move |ev: Event| {
        let Some(t) = ev.target() else { return };
        let Ok(input) = t.dyn_into::<HtmlInputElement>() else { return };
        if let Some(files) = input.files() {
            for i in 0..files.length() {
                if let Some(file) = files.item(i) {
                    let name = file.name();
                    let raw_mime = file.type_();
                    let mime = if raw_mime.is_empty() {
                        "application/octet-stream".to_string()
                    } else {
                        raw_mime
                    };
                    spawn_local(async move {
                        if let Ok(bytes) = read_file_bytes(&file).await {
                            pending.update(|p| p.push(PendingAttachment { name, mime, bytes }));
                        }
                    });
                }
            }
        }
        input.set_value("");
    };
    view! {
        <div class="msgr-composer-wrap">
            {move || {
                let items = pending.get();
                if items.is_empty() {
                    ().into_any()
                } else {
                    view! {
                        <div class="msgr-pending">
                            {items
                                .into_iter()
                                .enumerate()
                                .map(|(i, f)| {
                                    view! {
                                        <span class="msgr-pending-chip">
                                            "📎 "{f.name}
                                            <button
                                                type="button"
                                                class="msgr-pending-x"
                                                aria-label="Remove attachment"
                                                on:click=move |_| {
                                                    pending
                                                        .update(|p| {
                                                            if i < p.len() {
                                                                p.remove(i);
                                                            }
                                                        })
                                                }
                                            >
                                                "×"
                                            </button>
                                        </span>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                        .into_any()
                }
            }}
            <div class="msgr-composer">
                <label class="msgr-attach-btn" aria-label="Attach file" title="Attach file">
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
                    </svg>
                    <input
                        type="file"
                        multiple
                        style="display:none"
                        on:change=on_file
                    />
                </label>
                <textarea
                    class="msgr-composer-input"
                    rows="1"
                    placeholder="Write a message…"
                    prop:value=move || composer.get()
                    on:input=on_input
                    on:keydown={
                        let send = send.clone();
                        move |ev: KeyboardEvent| {
                            // Enter sends; Shift+Enter inserts a newline.
                            if ev.key() == "Enter" && !ev.shift_key() {
                                ev.prevent_default();
                                send();
                            }
                        }
                    }
                ></textarea>
                <button
                    type="button"
                    class="msgr-send-btn"
                    aria-label="Send"
                    on:click={
                        let send = send.clone();
                        move |_| send()
                    }
                    prop:disabled=move || {
                        busy.get() || (composer.get().trim().is_empty() && pending.get().is_empty())
                    }
                >
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="m22 2-7 20-4-9-9-4Z" />
                        <path d="M22 2 11 13" />
                    </svg>
                </button>
            </div>
        </div>
    }
}

// ── LinkPhoneModal ───────────────────────────────────────────────────────
// QR the Hey phone app scans to sign in (inherits this device's wallet
// session — no password). Encodes heyapp://connect?host=&app=hey-chat&token=
// via device_link_url; rendered with the shared invite_qr_svg.
#[component]
fn LinkPhoneModal(open: RwSignal<bool>) -> impl IntoView {
    // Auto-rotate the QR every 60s so the on-screen code stays fresh — the
    // device-link token self-expires (~120s), so a stale screenshot lapses.
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        spawn_local(async move {
            loop {
                wait_ms(60_000).await;
                tick.update(|t| *t += 1);
            }
        });
    });
    view! {
        <Show when=move || open.get() fallback=|| view! { <></> }>
            <div class="msgr-modal-backdrop" on:click=move |_: MouseEvent| open.set(false)>
                <div class="msgr-modal" on:click=|ev: MouseEvent| ev.stop_propagation()>
                    <header class="msgr-modal-header">
                        <h3 class="msgr-modal-title">"Link phone"</h3>
                        <button
                            type="button"
                            class="msgr-modal-close"
                            aria-label="Close"
                            on:click=move |_| open.set(false)
                        >
                            <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M18 6 6 18M6 6l12 12" />
                            </svg>
                        </button>
                    </header>
                    <div class="msgr-modal-body" style="text-align:center">
                        {move || {
                            tick.get();
                            match device_link_url("hey-chat").and_then(|l| invite_qr_svg(&l)) {
                                Some(svg) => view! {
                                    <div
                                        style="margin:0 auto;width:fit-content;background:#fff;padding:12px;border-radius:12px"
                                        inner_html=svg
                                    ></div>
                                }.into_any(),
                                None => view! {
                                    <p class="msgr-modal-hint">"Sign in first, then link your phone."</p>
                                }.into_any(),
                            }
                        }}
                        <p class="msgr-modal-hint">
                            "Open Hey on your phone and scan this — no password. The code refreshes every minute and expires shortly after, so scan it now and don't share a screenshot."
                        </p>
                    </div>
                </div>
            </div>
        </Show>
    }
}

// ── NetworkSettingsModal (peer-provider P2P settings) ─────────────────────
// Same panel as hey-social's, against the shared runtime peer node. "Always-on"
// background listening is automatic (the provider re-subscribes from disk on
// spawn) — this panel only exposes the advanced knobs: independent mode (direct,
// no relay) vs zero-config, a fixed UDP port, and the public address advertised
// in the shareable ticket.
#[component]
fn NetworkSettingsModal(open: RwSignal<bool>) -> impl IntoView {
    let loaded = RwSignal::new(false);
    let node_id = RwSignal::new(String::new());
    let ticket = RwSignal::new(String::new());
    let independent = RwSignal::new(false);
    let bind_port = RwSignal::new(String::new());
    let public_addr = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let msg = RwSignal::new(String::new());
    let copied = RwSignal::new(false);

    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        loaded.set(false);
        spawn_local(async move {
            if let Some(c) = hey_core::runtime::peer::get_config().await {
                node_id.set(c.node_id);
                ticket.set(c.ticket);
                independent.set(c.relay_mode == "independent" || c.running_independent);
                bind_port.set(if c.bind_port == 0 { String::new() } else { c.bind_port.to_string() });
                public_addr.set(c.public_addr);
            }
            loaded.set(true);
        });
    });

    let save = StoredValue::new(move || {
        if busy.get() {
            return;
        }
        busy.set(true);
        msg.set(String::new());
        let mode = if independent.get() { "independent" } else { "default" };
        let port: u32 = bind_port.get().trim().parse().unwrap_or(0);
        let pa = public_addr.get().trim().to_string();
        spawn_local(async move {
            match hey_core::runtime::peer::set_config(mode, port, &pa).await {
                Ok(true) => msg.set("Saved — restart the runtime to apply the mode/port change.".into()),
                Ok(false) => msg.set("Saved.".into()),
                Err(e) => msg.set(format!("Error: {e}")),
            }
            busy.set(false);
        });
    });
    let copy_ticket = StoredValue::new(move || {
        let t = ticket.get();
        if t.is_empty() {
            return;
        }
        if let Some(win) = web_sys::window() {
            let _ = win.navigator().clipboard().write_text(&t);
            copied.set(true);
        }
    });

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div class="msgr-modal-backdrop" on:click=move |_: MouseEvent| open.set(false)>
                <div class="msgr-modal" on:click=|ev: MouseEvent| ev.stop_propagation()>
                    <header class="msgr-modal-header">
                        <h3 class="msgr-modal-title">"Network / P2P"</h3>
                        <button type="button" class="msgr-modal-close" aria-label="Close" on:click=move |_| open.set(false)>
                            <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
                        </button>
                    </header>
                    <div class="msgr-modal-body">
                        {move || if !loaded.get() {
                            view! { <p class="msgr-modal-hint">"Loading…"</p> }.into_any()
                        } else if node_id.get().is_empty() {
                            view! { <p class="msgr-modal-hint">"Peer node not available on this runtime (same-runtime delivery only). Update the runtime to enable P2P."</p> }.into_any()
                        } else {
                            view! {
                                <>
                                    <p class="msgr-modal-hint">"Node: "{move || node_id.get()}</p>
                                    <label class="msgr-anon-toggle">
                                        <input type="checkbox" prop:checked=move || independent.get()
                                            on:change=move |ev: Event| { if let Some(t) = ev.target() { if let Ok(i) = t.dyn_into::<HtmlInputElement>() { independent.set(i.checked()); } } } />
                                        <span>"Independent mode (direct, no relay) — set a fixed port + public address peers can reach"</span>
                                    </label>
                                    <input class="msgr-invite-text" type="number" placeholder="Fixed UDP port (blank = automatic)"
                                        prop:value=move || bind_port.get()
                                        on:input=move |ev: Event| { if let Some(t) = ev.target() { if let Ok(i) = t.dyn_into::<HtmlInputElement>() { bind_port.set(i.value()); } } } />
                                    <input class="msgr-invite-text" type="text" placeholder="Public address host:port (advertised in your ticket)"
                                        prop:value=move || public_addr.get()
                                        on:input=move |ev: Event| { if let Some(t) = ev.target() { if let Ok(i) = t.dyn_into::<HtmlInputElement>() { public_addr.set(i.value()); } } } />
                                    <button type="button" class="msgr-btn-primary" on:click=move |_| save.with_value(|f| f()) prop:disabled=move || busy.get()>
                                        {move || if busy.get() { "Saving…" } else { "Save" }}
                                    </button>
                                    <div class="msgr-invite-box">
                                        <textarea class="msgr-invite-text" readonly=true prop:value=move || ticket.get()></textarea>
                                        <button type="button" class="msgr-btn-secondary" on:click=move |_| copy_ticket.with_value(|f| f())>
                                            {move || if copied.get() { "Copied!" } else { "Copy node ticket" }}
                                        </button>
                                    </div>
                                    {move || { let m = msg.get(); if m.is_empty() { ().into_any() } else { view! { <p class="msgr-modal-hint">{m}</p> }.into_any() } }}
                                </>
                            }.into_any()
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}

// ── AddContactModal ──────────────────────────────────────────────────────
#[component]
fn AddContactModal(open: RwSignal<bool>) -> impl IntoView {
    // Tab: "create" | "accept".
    let tab = RwSignal::new("create".to_string());
    let invite_link = RwSignal::new(String::new());
    let paste = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let copied = RwSignal::new(false);
    // Per-contact identity mode: false = Regular (stable, federated did:key),
    // true = Anonymous (fresh per-contact ephemeral identity — incognito).
    let anon = RwSignal::new(false);
    let navigate = use_navigate();

    // Handlers are stashed in StoredValue so they're `Copy` and can be used
    // freely inside the (re-runnable) `<Show>` children + the reactive tab
    // blocks without move/FnOnce conflicts.
    let do_generate = StoredValue::new(move || {
        if busy.get() {
            return;
        }
        error.set(String::new());
        busy.set(true);
        let mode = if anon.get() { IdentityMode::Anonymous } else { IdentityMode::Regular };
        spawn_local(async move {
            match generate_invite("", mode).await {
                Ok(link) => invite_link.set(link),
                Err(e) => error.set(e),
            }
            busy.set(false);
        });
    });

    let do_accept = StoredValue::new({
        let navigate = navigate.clone();
        move || {
            if busy.get() {
                return;
            }
            let token = paste.get().trim().to_string();
            if token.is_empty() {
                return;
            }
            error.set(String::new());
            busy.set(true);
            let mode = if anon.get() { IdentityMode::Anonymous } else { IdentityMode::Regular };
            let navigate = navigate.clone();
            spawn_local(async move {
                match accept_invite(&token, mode).await {
                    Ok(did) => {
                        paste.set(String::new());
                        open.set(false);
                        navigate(&format!("/chat/{}", did), NavigateOptions::default());
                    }
                    Err(e) => error.set(e),
                }
                busy.set(false);
            });
        }
    });

    let copy_link = StoredValue::new(move || {
        let link = invite_link.get();
        if link.is_empty() {
            return;
        }
        if let Some(win) = web_sys::window() {
            let clipboard = win.navigator().clipboard();
            let _ = clipboard.write_text(&link);
            copied.set(true);
        }
    });

    // Escape-to-close. Bind the window keydown listener ONCE for this
    // modal's lifetime. The handler uses disposal-safe `try_*` accessors so
    // a forgotten closure no-ops (instead of crashing) once the modal's
    // reactive owner is gone. (Previously this Effect re-added + `.forget()`'d
    // a fresh listener on every `open` toggle; those leaked closures fired
    // `open.set` into a disposed signal after unmount → "closure invoked
    // recursively or after being dropped" — the WASM error in the console.)
    if let Some(win) = web_sys::window() {
        let closure: wasm_bindgen::closure::Closure<dyn FnMut(KeyboardEvent)> =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: KeyboardEvent| {
                if ev.key() == "Escape" && open.try_get_untracked() == Some(true) {
                    let _ = open.try_set(false);
                }
            }));
        let _ =
            win.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // Reset transient state every time the modal opens.
    Effect::new(move |_| {
        if open.get() {
            error.set(String::new());
            copied.set(false);
        }
    });

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div
                class="msgr-modal-backdrop"
                on:click=move |_: MouseEvent| open.set(false)
            >
                <div
                    class="msgr-modal"
                    on:click=|ev: MouseEvent| ev.stop_propagation()
                >
                    <header class="msgr-modal-header">
                        <h3 class="msgr-modal-title">"Add contact"</h3>
                        <button
                            type="button"
                            class="msgr-modal-close"
                            aria-label="Close"
                            on:click=move |_| open.set(false)
                        >
                            <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M18 6 6 18M6 6l12 12" />
                            </svg>
                        </button>
                    </header>

                    <div class="msgr-tabs">
                        <button
                            type="button"
                            class=move || if tab.get() == "create" { "msgr-tab msgr-tab-active" } else { "msgr-tab" }
                            on:click=move |_| tab.set("create".into())
                        >
                            "Create invite"
                        </button>
                        <button
                            type="button"
                            class=move || if tab.get() == "accept" { "msgr-tab msgr-tab-active" } else { "msgr-tab" }
                            on:click=move |_| tab.set("accept".into())
                        >
                            "Accept invite"
                        </button>
                    </div>

                    <label class="msgr-anon-toggle">
                        <input
                            type="checkbox"
                            prop:checked=move || anon.get()
                            on:change=move |ev: Event| {
                                if let Some(t) = ev.target() {
                                    if let Ok(i) = t.dyn_into::<HtmlInputElement>() {
                                        anon.set(i.checked());
                                    }
                                }
                            }
                        />
                        <span>"Anonymous (incognito) — present a throwaway identity to this contact"</span>
                    </label>

                    {move || if tab.get() == "create" {
                        view! {
                            <div class="msgr-modal-body">
                                <p class="msgr-modal-hint">
                                    "Mint a one-time invite link and share it with someone. When they paste it back, you'll appear in each other's chats."
                                </p>
                                <button
                                    type="button"
                                    class="msgr-btn-primary"
                                    on:click=move |_| do_generate.with_value(|f| f())
                                    prop:disabled=move || busy.get()
                                >
                                    {move || if busy.get() { "Generating…" } else { "Generate invite link" }}
                                </button>
                                {move || {
                                    let link = invite_link.get();
                                    if link.is_empty() {
                                        ().into_any()
                                    } else {
                                        view! {
                                            <div class="msgr-invite-box">
                                                <textarea
                                                    class="msgr-invite-text"
                                                    readonly=true
                                                    prop:value=link.clone()
                                                ></textarea>
                                                <button
                                                    type="button"
                                                    class="msgr-btn-secondary"
                                                    on:click=move |_| copy_link.with_value(|f| f())
                                                >
                                                    {move || if copied.get() { "Copied!" } else { "Copy link" }}
                                                </button>
                                            </div>
                                        }.into_any()
                                    }
                                }}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="msgr-modal-body">
                                <p class="msgr-modal-hint">
                                    "Paste an invite link someone shared with you to start chatting."
                                </p>
                                <textarea
                                    class="msgr-invite-text"
                                    placeholder="hey-invite:…"
                                    prop:value=move || paste.get()
                                    on:input=move |ev: Event| {
                                        if let Some(t) = ev.target() {
                                            if let Ok(i) = t.dyn_into::<HtmlTextAreaElement>() {
                                                paste.set(i.value());
                                            }
                                        }
                                    }
                                    on:keydown=move |ev: KeyboardEvent| {
                                        if ev.key() == "Enter" && !ev.shift_key() {
                                            ev.prevent_default();
                                            do_accept.with_value(|f| f());
                                        }
                                    }
                                ></textarea>
                                <button
                                    type="button"
                                    class="msgr-btn-primary"
                                    on:click=move |_| do_accept.with_value(|f| f())
                                    prop:disabled=move || busy.get() || paste.get().trim().is_empty()
                                >
                                    {move || if busy.get() { "Accepting…" } else { "Accept invite" }}
                                </button>
                            </div>
                        }.into_any()
                    }}

                    {move || {
                        let m = error.get();
                        if m.is_empty() {
                            ().into_any()
                        } else {
                            view! { <p class="msgr-error msgr-modal-error">{m}</p> }.into_any()
                        }
                    }}
                </div>
            </div>
        </Show>
    }
}

// ── Avatar ───────────────────────────────────────────────────────────────
#[component]
fn Avatar(name: String) -> impl IntoView {
    let letters = initial_letters(&name);
    view! {
        <div class="msgr-avatar">{letters}</div>
    }
}

// ── Group chat UI ─────────────────────────────────────────────────────────

/// Reusable contact list with search + multi-select checkboxes. The "contacts
/// list" the user picks group members from. `exclude` hides DIDs already in.
#[component]
fn ContactPicker(
    contacts: RwSignal<Vec<DmContact>>,
    selected: RwSignal<Vec<String>>,
    search: RwSignal<String>,
    exclude: RwSignal<Vec<String>>,
) -> impl IntoView {
    let toggle = move |did: String| {
        selected.update(|s| {
            if let Some(i) = s.iter().position(|d| *d == did) {
                s.remove(i);
            } else {
                s.push(did);
            }
        });
    };
    view! {
        <input
            type="text"
            placeholder="Search contacts…"
            prop:value=move || search.get()
            on:input=move |ev| search.set(event_target_value(&ev))
            style="width:100%;box-sizing:border-box;padding:7px 12px;border-radius:9px;border:1px solid rgba(127,127,127,0.2);background:rgba(127,127,127,0.06);color:inherit;font-size:14px;outline:none;margin-bottom:8px;"
        />
        <div style="max-height:42vh;overflow:auto;display:flex;flex-direction:column;gap:2px;">
            <For
                each=move || {
                    let ex = exclude.get();
                    let active: Vec<DmContact> = contacts.get().into_iter()
                        .filter(|c| c.is_v2_active() && !c.did.starts_with("pending:") && !ex.contains(&c.did))
                        .collect();
                    filtered_contacts(&active, &search.get())
                }
                key=|c| c.did.clone()
                children={
                    let toggle = toggle.clone();
                    move |c: DmContact| {
                        let toggle = toggle.clone();
                        let did = c.did.clone();
                        let did_chk = c.did.clone();
                        let nm = display_name(&c);
                        view! {
                            <button
                                type="button"
                                on:click=move |_| toggle(did.clone())
                                style="display:flex;align-items:center;gap:10px;width:100%;border:none;background:transparent;text-align:left;padding:6px 8px;border-radius:8px;cursor:pointer;color:inherit;"
                            >
                                <Avatar name=nm.clone() />
                                <span style="flex:1;">{nm.clone()}</span>
                                <span style="font-size:18px;">
                                    {move || if selected.get().iter().any(|d| *d == did_chk) { "\u{2611}" } else { "\u{2610}" }}
                                </span>
                            </button>
                        }
                    }
                }
            />
        </div>
    }
}

#[component]
fn NewGroupModal(open: RwSignal<bool>, contacts: RwSignal<Vec<DmContact>>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let selected: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let search = RwSignal::new(String::new());
    let exclude: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let navigate = use_navigate();

    let create = StoredValue::new({
        let navigate = navigate.clone();
        move || {
            if busy.get() {
                return;
            }
            let members = selected.get();
            if members.is_empty() {
                error.set("Pick at least one member.".into());
                return;
            }
            let nm = {
                let n = name.get().trim().to_string();
                if n.is_empty() { "Group".to_string() } else { n }
            };
            error.set(String::new());
            busy.set(true);
            let navigate = navigate.clone();
            spawn_local(async move {
                match create_group(&nm, members).await {
                    Ok(gid) => {
                        name.set(String::new());
                        selected.set(Vec::new());
                        search.set(String::new());
                        open.set(false);
                        navigate(&format!("/group/{}", gid), NavigateOptions::default());
                    }
                    Err(e) => error.set(e),
                }
                busy.set(false);
            });
        }
    });

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div class="msgr-modal-backdrop" on:click=move |_: MouseEvent| open.set(false)>
                <div class="msgr-modal" on:click=|ev: MouseEvent| ev.stop_propagation()>
                    <header class="msgr-modal-header">
                        <h3 class="msgr-modal-title">"New group"</h3>
                        <button type="button" class="msgr-modal-close" aria-label="Close" on:click=move |_| open.set(false)>
                            <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
                        </button>
                    </header>
                    <div class="msgr-modal-body">
                        <input
                            type="text"
                            placeholder="Group name"
                            prop:value=move || name.get()
                            on:input=move |ev| name.set(event_target_value(&ev))
                            style="width:100%;box-sizing:border-box;padding:9px 12px;border-radius:9px;border:1px solid rgba(127,127,127,0.25);background:rgba(127,127,127,0.06);color:inherit;font-size:15px;outline:none;margin-bottom:8px;"
                        />
                        <ContactPicker contacts=contacts selected=selected search=search exclude=exclude />
                        {move || {
                            let e = error.get();
                            (!e.is_empty()).then(|| view! { <p style="color:#e5484d;font-size:13px;margin:6px 0 0;">{e}</p> })
                        }}
                        <button
                            type="button"
                            on:click=move |_| create.with_value(|f| f())
                            disabled=move || busy.get() || selected.get().is_empty()
                            style="width:100%;margin-top:10px;padding:11px;border-radius:9px;border:none;background:#6d6ef5;color:#fff;font-size:15px;font-weight:600;cursor:pointer;"
                        >
                            {move || {
                                let n = selected.get().len();
                                if busy.get() { "Creating…".to_string() }
                                else if n == 0 { "Create group".to_string() }
                                else { format!("Create group · {n}") }
                            }}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn AddMembersModal(open: RwSignal<bool>, gid: String) -> impl IntoView {
    let contacts: RwSignal<Vec<DmContact>> = RwSignal::new(Vec::new());
    let selected: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let search = RwSignal::new(String::new());
    let existing: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    let busy = RwSignal::new(false);

    {
        let gid = gid.clone();
        Effect::new(move |_| {
            if !open.get() {
                return;
            }
            let gid = gid.clone();
            spawn_local(async move {
                contacts.set(list_contacts().await);
                if let Some(g) = list_groups().await.into_iter().find(|g| g.id == gid) {
                    existing.set(g.members.into_iter().map(|m| m.did).collect());
                }
            });
        });
    }

    let gid_add = gid.clone();
    let add = StoredValue::new(move || {
        if busy.get() {
            return;
        }
        let members = selected.get();
        if members.is_empty() {
            return;
        }
        busy.set(true);
        let gid = gid_add.clone();
        spawn_local(async move {
            let _ = add_group_members(&gid, members).await;
            selected.set(Vec::new());
            busy.set(false);
            open.set(false);
        });
    });

    view! {
        <Show when=move || open.get() fallback=|| ().into_view()>
            <div class="msgr-modal-backdrop" on:click=move |_: MouseEvent| open.set(false)>
                <div class="msgr-modal" on:click=|ev: MouseEvent| ev.stop_propagation()>
                    <header class="msgr-modal-header">
                        <h3 class="msgr-modal-title">"Add members"</h3>
                        <button type="button" class="msgr-modal-close" aria-label="Close" on:click=move |_| open.set(false)>
                            <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
                        </button>
                    </header>
                    <div class="msgr-modal-body">
                        <ContactPicker contacts=contacts selected=selected search=search exclude=existing />
                        <button
                            type="button"
                            on:click=move |_| add.with_value(|f| f())
                            disabled=move || busy.get() || selected.get().is_empty()
                            style="width:100%;margin-top:10px;padding:11px;border-radius:9px;border:none;background:#6d6ef5;color:#fff;font-size:15px;font-weight:600;cursor:pointer;"
                        >
                            {move || {
                                let n = selected.get().len();
                                if busy.get() { "Adding…".to_string() }
                                else if n == 0 { "Add to group".to_string() }
                                else { format!("Add {n} to group") }
                            }}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn GroupConversation(gid: String) -> impl IntoView {
    let messages: RwSignal<Vec<DmMessage>> = RwSignal::new(Vec::new());
    let composer = RwSignal::new(String::new());
    let pending: RwSignal<Vec<PendingAttachment>> = RwSignal::new(Vec::new());
    let busy = RwSignal::new(false);
    let title = RwSignal::new(String::new());
    let member_count = RwSignal::new(0usize);
    let add_open = RwSignal::new(false);

    {
        let g = gid.clone();
        Effect::new(move |_| {
            let g = g.clone();
            spawn_local(async move {
                loop {
                    messages.set(read_group_conversation(&g).await);
                    if let Some(grp) = list_groups().await.into_iter().find(|x| x.id == g) {
                        title.set(grp.name.clone());
                        member_count.set(grp.members.len());
                    }
                    mark_group_read(&g).await;
                    wait_ms(2500).await;
                }
            });
        });
    }

    let send = {
        let g = gid.clone();
        move || {
            if busy.get() {
                return;
            }
            let text = composer.get();
            let files = pending.get();
            if text.trim().is_empty() && files.is_empty() {
                return;
            }
            let g = g.clone();
            busy.set(true);
            spawn_local(async move {
                composer.set(String::new());
                pending.set(Vec::new());
                if files.is_empty() {
                    let _ = send_group_message(&g, &text).await;
                } else {
                    let mut atts = Vec::new();
                    for f in &files {
                        if let Ok(a) = upload_attachment(&f.name, &f.mime, &f.bytes).await {
                            atts.push(a);
                        }
                    }
                    let _ = send_group_message_with_attachments(&g, &text, atts).await;
                }
                messages.set(read_group_conversation(&g).await);
                busy.set(false);
            });
        }
    };

    view! {
        <div class="msgr-conv">
            <header class="msgr-conv-header">
                <div class="msgr-avatar" style="display:flex;align-items:center;justify-content:center;font-size:18px;background:linear-gradient(135deg,#6d6ef5,#a06df5);color:#fff;">"👥"</div>
                <div class="msgr-conv-title">
                    <span class="msgr-conv-name">{move || title.get()}</span>
                    <span
                        class="msgr-conv-status"
                        title="Every group message is sealed PER-MEMBER (post-quantum sealed-sender + Double Ratchet where available) — there is no shared group key, so each link keeps its own forward secrecy."
                    >
                        {move || format!("{} members \u{00b7} \u{1f512} end-to-end encrypted", member_count.get())}
                    </span>
                </div>
                <button
                    type="button"
                    class="msgr-add-btn"
                    title="Add members"
                    aria-label="Add members"
                    style="margin-left:auto;"
                    on:click=move |_| add_open.set(true)
                >
                    <svg viewBox="0 0 24 24" class="msgr-icon" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
                        <circle cx="9" cy="7" r="4" />
                        <path d="M19 8v6M22 11h-6" />
                    </svg>
                </button>
            </header>

            <div class="msgr-conv-body">
                {move || {
                    let list = messages.get();
                    if list.is_empty() {
                        view! { <div class="msgr-conv-empty"><p>"No messages yet. Say hi to the group 👋"</p></div> }.into_any()
                    } else {
                        view! {
                            <For each=move || messages.get() key=|m| m.id.clone()
                                children=move |m: DmMessage| view! { <Bubble m=m /> } />
                        }.into_any()
                    }
                }}
            </div>

            <Composer composer=composer pending=pending busy=busy send=send.clone() />
            <AddMembersModal open=add_open gid=gid.clone() />
        </div>
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Filter groups by a case-insensitive name query (empty query = all).
fn filtered_groups(groups: &[Group], q: &str) -> Vec<Group> {
    let q = q.trim().to_lowercase();
    groups
        .iter()
        .filter(|g| q.is_empty() || g.name.to_lowercase().contains(&q))
        .cloned()
        .collect()
}

/// Filter contacts by a case-insensitive name/did query (empty query = all).
fn filtered_contacts(contacts: &[DmContact], q: &str) -> Vec<DmContact> {
    let q = q.trim().to_lowercase();
    contacts
        .iter()
        .filter(|c| q.is_empty() || display_name(c).to_lowercase().contains(&q) || c.did.to_lowercase().contains(&q))
        .cloned()
        .collect()
}

fn display_name(c: &DmContact) -> String {
    if !c.name.is_empty() {
        return c.name.clone();
    }
    if c.did.starts_with("pending:") {
        return "Awaiting reply…".into();
    }
    short_did(&c.did)
}

fn short_did(did: &str) -> String {
    if did.starts_with("pending:") {
        return "(invite pending)".into();
    }
    let s = did.strip_prefix("did:key:z").unwrap_or(did);
    if s.len() > 12 {
        format!("{}…", s.chars().take(12).collect::<String>())
    } else {
        s.into()
    }
}

fn initial_letters(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect::<String>()
        .to_uppercase();
    if s.is_empty() {
        "?".into()
    } else {
        s
    }
}

fn ts_short(ts: i64) -> String {
    if ts == 0 {
        return String::new();
    }
    let now = js_sys::Date::now();
    let diff_secs = ((now - ts as f64) / 1000.0).max(0.0) as i64;
    if diff_secs < 60 {
        return "now".into();
    }
    let mins = diff_secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts as f64));
    d.to_locale_date_string("en-US", &wasm_bindgen::JsValue::UNDEFINED)
        .as_string()
        .unwrap_or_default()
}

async fn wait_ms(ms: i32) {
    let win = web_sys::window().unwrap();
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}
