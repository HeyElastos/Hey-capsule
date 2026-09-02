//! Chat — the conversation list, the thread, and the composer.
//!
//! Engine calls stay on `hey_core::api::dms`. Speaker-runs group by
//! `sender_did` so two people in a group do not read as one. Attachments
//! are all-or-nothing: if a file cannot be read, nothing sends.

use hey_core::api::dms;
use leptos::prelude::*;
use leptos::task::spawn_local;

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
    sender_did: String,
    sender_name: String,
    attachments: usize,
}

#[derive(Clone, PartialEq)]
struct ChatRun {
    key: String,
    sender_did: String,
    name: String,
    mine: bool,
    lines: Vec<Msg>,
}

fn to_msg(m: &dms::DmMessage) -> Msg {
    Msg {
        id: m.id.clone(),
        text: m.text.clone(),
        mine: m.mine,
        ts: m.ts,
        encrypted: m.encrypted,
        sender_did: m.sender_did.clone(),
        sender_name: m.sender_name.clone(),
        attachments: m.attachments.len(),
    }
}

fn visible(ms: &[dms::DmMessage]) -> Vec<Msg> {
    ms.iter()
        .filter(|m| !m.text.starts_with('\u{1}'))
        .map(to_msg)
        .collect()
}

fn group_chat(msgs: Vec<Msg>) -> Vec<ChatRun> {
    let mut runs: Vec<ChatRun> = Vec::new();
    for m in msgs {
        let did = if m.sender_did.is_empty() {
            if m.mine {
                "me".into()
            } else {
                m.id.clone()
            }
        } else {
            m.sender_did.clone()
        };
        let name = if !m.sender_name.is_empty() {
            m.sender_name.clone()
        } else if m.mine {
            "You".into()
        } else {
            crate::shorten_did(&did)
        };
        if let Some(last) = runs.last_mut() {
            if last.sender_did == did {
                last.lines.push(m);
                continue;
            }
        }
        runs.push(ChatRun {
            key: m.id.clone(),
            sender_did: did,
            name,
            mine: m.mine,
            lines: vec![m],
        });
    }
    runs
}

#[component]
pub fn Chat() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Row>::new());
    let (open, set_open) = signal(String::new());
    let (thread, set_thread) = signal(Vec::<Msg>::new());
    let (loaded, set_loaded) = signal(false);
    let divider_after = RwSignal::new(String::new());
    let draft = RwSignal::new(String::new());
    let pending_files = RwSignal::new(Vec::<web_sys::File>::new());
    let attach_err = RwSignal::new(String::new());

    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    on_cleanup({
        let alive = alive.clone();
        move || alive.store(false, std::sync::atomic::Ordering::Relaxed)
    });

    spawn_local(async move {
        while alive.load(std::sync::atomic::Ordering::Relaxed) {
            let cs = dms::list_contacts().await;
            set_rows.set(
                cs.iter()
                    .map(|c| Row {
                        did: c.did.clone(),
                        name: if c.name.is_empty() {
                            crate::shorten_did(&c.did)
                        } else {
                            c.name.clone()
                        },
                        preview: c.last_preview.clone(),
                        unread: c.unread,
                    })
                    .collect(),
            );
            set_loaded.set(true);
            let cur = open.get_untracked();
            if !cur.is_empty() {
                set_thread.set(visible(&dms::read_conversation(&cur).await));
            }
            gloo_timers::future::TimeoutFuture::new(POLL_MS).await;
        }
    });

    let pick = move |did: String| {
        set_open.set(did.clone());
        set_thread.set(Vec::new());
        divider_after.set(crate::prefs::last_read_id(&did).unwrap_or_default());
        spawn_local(async move {
            dms::mark_read(&did).await;
            let msgs = visible(&dms::read_conversation(&did).await);
            if let Some(last) = msgs.last() {
                crate::prefs::set_last_read_id(&did, &last.id);
            }
            set_thread.set(msgs);
        });
    };

    let send = move || {
        let text = draft.get_untracked().trim().to_string();
        let did = open.get_untracked();
        let files = pending_files.get_untracked();
        if (text.is_empty() && files.is_empty()) || did.is_empty() {
            return;
        }
        draft.set(String::new());
        pending_files.set(Vec::new());
        attach_err.set(String::new());
        spawn_local(async move {
            if files.is_empty() {
                match dms::send_message(&did, &text).await {
                    Ok(m) => set_thread.update(|t| t.push(to_msg(&m))),
                    Err(e) => leptos::logging::warn!("send failed: {e}"),
                }
                return;
            }
            match crate::media::read_all_or_none(&files).await {
                Ok(parts) => {
                    let mut atts = Vec::new();
                    for (name, mime, bytes) in parts {
                        match dms::upload_attachment(&name, &mime, &bytes).await {
                            Ok(a) => atts.push(a),
                            Err(e) => {
                                attach_err.set(format!("{name}: {e}. Nothing was sent."));
                                return;
                            }
                        }
                    }
                    match dms::send_message_with_attachments(&did, &text, atts).await {
                        Ok(m) => set_thread.update(|t| t.push(to_msg(&m))),
                        Err(e) => attach_err.set(e),
                    }
                }
                Err(e) => attach_err.set(format!("{e}. Nothing was sent.")),
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
                <Show when=move || loaded.get() && rows.get().is_empty() fallback=|| ().into_view()>
                    <p class="empty">
                        "No conversations yet. Accept an invite link, or send one, and it appears here."
                    </p>
                </Show>
                <For each=move || rows.get() key=|r| (r.did.clone(), r.unread, r.preview.clone()) let:r>
                    {
                        let did = r.did.clone();
                        let is_open = {
                            let d = did.clone();
                            move || open.get() == d
                        };
                        view! {
                            <button class="row-btn" class:active=is_open on:click=move |_| pick(did.clone())>
                                <span class="avatar" style=crate::tint_css(&r.did)>{initial(&r.name)}</span>
                                <span class="who">
                                    <b>{r.name.clone()}</b>
                                    <i>{r.preview.clone()}</i>
                                </span>
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
                                .unwrap_or_else(|| crate::shorten_did(&d))
                        }
                    }}
                </h1>
                <div class="spring"></div>
                <Show when=move || !open.get().is_empty() fallback=|| ().into_view()>
                    <span class="chip good">"Encrypted"</span>
                </Show>
            </header>

            <div class="body thread">
                <Show when=move || open.get().is_empty() fallback=|| ().into_view()>
                    <p class="empty">"Pick a conversation."</p>
                </Show>
                <For each=move || group_chat(thread.get()) key=|r| r.key.clone() let:r>
                    {
                        let nm = r.name.clone();
                        let ids: Vec<String> = r.lines.iter().map(|l| l.id.clone()).collect();
                        let is_new_run = {
                            let ids = ids.clone();
                            move || {
                                let mark = divider_after.get();
                                !mark.is_empty() && ids.iter().any(|id| id == &mark)
                            }
                        };
                        view! {
                            <div class="run" class:mine=move || r.mine style=crate::tint_css(&r.sender_did)>
                                <Show when=move || !r.mine fallback=|| ().into_view()>
                                    <span class="avatar">{initial(&nm)}</span>
                                </Show>
                                <div>
                                    <Show when=move || !r.mine fallback=|| ().into_view()>
                                        <b class="run-name">{r.name.clone()}</b>
                                    </Show>
                                    <For each=move || r.lines.clone() key=|l| l.id.clone() let:l>
                                        <div class="bubble" class:mine=move || r.mine>
                                            <span>{l.text.clone()}</span>
                                            <Show when=move || (l.attachments > 0) fallback=|| ().into_view()>
                                                <i class="sub">{l.attachments}" files"</i>
                                            </Show>
                                        </div>
                                    </For>
                                </div>
                            </div>
                            <Show when=is_new_run fallback=|| ().into_view()>
                                <div class="new-bar">"New"</div>
                            </Show>
                        }
                    }
                </For>
            </div>

            <Show when=move || !open.get().is_empty() fallback=|| ().into_view()>
                <footer class="composer">
                    <input type="file" multiple on:change=move |e| {
                        pending_files.set(crate::prefs::files_from_input(e));
                    } />
                    <textarea
                        placeholder="Message. Shift+Enter for a new line"
                        prop:value=move || draft.get()
                        on:input=move |e| draft.set(event_target_value(&e))
                        on:keydown=move |e| {
                            if e.key() == "Enter" && !e.shift_key() {
                                e.prevent_default();
                                send();
                            }
                        }
                    ></textarea>
                    <button class="send" on:click=move |_| send() title="Send">"\u{2191}"</button>
                </footer>
                <Show when=move || !pending_files.get().is_empty() fallback=|| ().into_view()>
                    <p class="note">{move || format!("{} file(s) attached. All of them must read or none send", pending_files.get().len())}</p>
                </Show>
                <Show when=move || !attach_err.get().is_empty() fallback=|| ().into_view()>
                    <p class="note">{move || attach_err.get()}</p>
                </Show>
            </Show>
        </section>
    }
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}
