//! Chat — the conversation list, the thread, and the composer.
//!
//! Every call here goes to `hey_core::api::dms`, the same module the desktop
//! and phone apps use. Not a reimplementation and not a compatible reimplementation
//! — literally the same code, compiled for wasm. That is what makes a
//! conversation started on the phone open here with its history intact: same
//! sealed-sender wire, same `dm/by-did/<did>.json` layout, same ratchet.
//!
//! The transport underneath differs and has to: natively the engine opens iroh
//! sockets, and in a browser tab it routes `provider_call` over the runtime's
//! HTTP. The DM layer does not know or care which, which is the whole reason
//! this file is short.

use hey_core::api::dms;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// How often the thread and the contact list re-read from the engine.
///
/// A poll and not a push, deliberately: the engine's receive path writes to
/// storage and there is no change signal to subscribe to from here. Two seconds
/// is under the threshold where a reply feels delayed in conversation, and the
/// read is local — it does not touch the network.
const POLL_MS: u32 = 2_000;

#[derive(Clone, PartialEq)]
struct Row {
    did: String,
    name: String,
    preview: String,
    unread: u32,
}

#[derive(Clone, PartialEq)]
struct Msg {
    id: String,
    text: String,
    mine: bool,
    ts: i64,
    encrypted: bool,
}

#[component]
pub fn Chat() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Row>::new());
    let (open, set_open) = signal(String::new());
    let (thread, set_thread) = signal(Vec::<Msg>::new());
    let (loaded, set_loaded) = signal(false);
    let draft = RwSignal::new(String::new());

    // STOPS WHEN THE TAB GOES AWAY.
    //
    // The shell rebuilds its pane on every tab change, so Chat unmounts and
    // remounts each time you leave and come back. An unconditional `loop` in
    // spawn_local survives that — the old one keeps polling forever and the new
    // one starts beside it, so N visits to Chat means N loops hammering storage
    // and racing each other's writes into the same signals. A plain flag flipped
    // on cleanup is enough; the future notices at its next tick and returns.
    // Arc/AtomicBool and not Rc/Cell: `on_cleanup` wants Send + Sync. wasm is
    // single-threaded, so the atomic costs nothing here — it is the bound that
    // needs satisfying, not the hardware.
    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    on_cleanup({
        let alive = alive.clone();
        move || alive.store(false, std::sync::atomic::Ordering::Relaxed)
    });

    // One poll drives both lists. It re-reads whatever conversation is open at
    // the time it fires rather than capturing one.
    spawn_local(async move {
        while alive.load(std::sync::atomic::Ordering::Relaxed) {
            let cs = dms::list_contacts().await;
            set_rows.set(
                cs.iter()
                    .map(|c| Row {
                        did: c.did.clone(),
                        name: if c.name.is_empty() { short(&c.did) } else { c.name.clone() },
                        preview: c.last_preview.clone(),
                        unread: c.unread,
                    })
                    .collect(),
            );
            set_loaded.set(true);

            let cur = open.get_untracked();
            if !cur.is_empty() {
                let ms = dms::read_conversation(&cur).await;
                set_thread.set(
                    ms.iter()
                        .map(|m| Msg {
                            id: m.id.clone(),
                            text: m.text.clone(),
                            mine: m.mine,
                            ts: m.ts,
                            encrypted: m.encrypted,
                        })
                        .collect(),
                );
            }
            gloo_timers::future::TimeoutFuture::new(POLL_MS).await;
        }
    });

    let pick = move |did: String| {
        set_open.set(did.clone());
        set_thread.set(Vec::new());
        spawn_local(async move {
            // Clearing the badge before the read, so the count does not flash
            // back on for one poll cycle after the thread is already on screen.
            dms::mark_read(&did).await;
            let ms = dms::read_conversation(&did).await;
            set_thread.set(
                ms.iter()
                    .map(|m| Msg {
                        id: m.id.clone(),
                        text: m.text.clone(),
                        mine: m.mine,
                        ts: m.ts,
                        encrypted: m.encrypted,
                    })
                    .collect(),
            );
        });
    };

    let send = move || {
        let text = draft.get_untracked().trim().to_string();
        let did = open.get_untracked();
        if text.is_empty() || did.is_empty() {
            return;
        }
        draft.set(String::new());
        spawn_local(async move {
            // OPTIMISTIC on the engine's answer, not on the press: send_message
            // returns the stored DmMessage, so appending its result keeps the
            // id and timestamp the engine assigned rather than inventing ones
            // the next poll would immediately contradict.
            match dms::send_message(&did, &text).await {
                Ok(m) => set_thread.update(|t| {
                    t.push(Msg {
                        id: m.id,
                        text: m.text,
                        mine: true,
                        ts: m.ts,
                        encrypted: m.encrypted,
                    })
                }),
                Err(e) => leptos::logging::warn!("send failed: {e}"),
            }
        });
    };

    view! {
        <section class="plane list-pane">
            <header class="bar">
                <h1>"Messages"</h1>
                <div class="spring"></div>
            </header>
            <div class="body list">
                <Show
                    when=move || loaded.get() && rows.get().is_empty()
                    fallback=|| ().into_view()
                >
                    <p class="empty">
                        "No conversations yet. Accept an invite link, or send one, and it appears here."
                    </p>
                </Show>
                <For
                    each=move || rows.get()
                    key=|r| (r.did.clone(), r.unread, r.preview.clone())
                    let:r
                >
                    {
                        let did = r.did.clone();
                        let is_open = {
                            let d = did.clone();
                            move || open.get() == d
                        };
                        view! {
                            <button
                                class="row-btn"
                                class:active=is_open
                                on:click=move |_| pick(did.clone())
                            >
                                <span class="avatar">{initial(&r.name)}</span>
                                <span class="who">
                                    <b>{r.name.clone()}</b>
                                    <i>{r.preview.clone()}</i>
                                </span>
                                // Parenthesised: the view! macro reads a bare `>` as a tag close.
                                <Show when=move || (r.unread > 0) fallback=|| ().into_view()>
                                    <span class="badge">{r.unread}</span>
                                </Show>
                            </button>
                        }
                    }
                </For>
            </div>
        </section>

        <section class="plane" style="flex:1">
            <header class="bar">
                <h1>
                    {move || {
                        let d = open.get();
                        if d.is_empty() {
                            "No conversation".to_string()
                        } else {
                            rows.get()
                                .iter()
                                .find(|r| r.did == d)
                                .map(|r| r.name.clone())
                                .unwrap_or_else(|| short(&d))
                        }
                    }}
                </h1>
                <div class="spring"></div>
                <Show when=move || !open.get().is_empty() fallback=|| ().into_view()>
                    <span class="chip good">"Encrypted"</span>
                </Show>
            </header>

            <div class="body thread">
                <Show
                    when=move || open.get().is_empty()
                    fallback=|| ().into_view()
                >
                    <p class="empty">"Pick a conversation."</p>
                </Show>
                <For each=move || thread.get() key=|m| m.id.clone() let:m>
                    <div class="bubble" class:mine=move || m.mine>
                        <span>{m.text.clone()}</span>
                    </div>
                </For>
            </div>

            <Show when=move || !open.get().is_empty() fallback=|| ().into_view()>
                <footer class="composer">
                    <input
                        placeholder="Message"
                        prop:value=move || draft.get()
                        on:input=move |e| draft.set(event_target_value(&e))
                        on:keydown=move |e| {
                            if e.key() == "Enter" {
                                e.prevent_default();
                                send();
                            }
                        }
                    />
                    <button class="send" on:click=move |_| send() title="Send">
                        "\u{2191}"
                    </button>
                </footer>
            </Show>
        </section>
    }
}

/// A did is long and the middle of it carries no information a person uses.
fn short(did: &str) -> String {
    let s = did.strip_prefix("did:key:").unwrap_or(did);
    // CHARS, not bytes. `&s[..8]` panics the moment the string is not ASCII,
    // and a display name very often is not — the desktop app has already paid
    // for this exact bug once. A did:key is ASCII and would never have shown it.
    let n = s.chars().count();
    if n <= 14 {
        return s.to_string();
    }
    let head: String = s.chars().take(8).collect();
    let tail: String = s.chars().skip(n - 4).collect();
    format!("{head}\u{2026}{tail}")
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}
