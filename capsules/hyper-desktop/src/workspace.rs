//! Workspaces — docs, presence, and a huddle that exists when you look away.
//!
//! Persist via runtime storage. Gossip via the peer provider. No iroh-docs
//! node. A huddle started here is the same group call as Calls.

use crate::calls;
use crate::prefs;
use hey_core::api::dms;
use hey_core::runtime::{peer, storage};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};

const POLL_MS: u32 = 1_200;

#[derive(Clone, PartialEq)]
struct Space {
    id: String,
    name: String,
    gid: String,
}

#[derive(Clone, PartialEq)]
struct Doc {
    id: String,
    title: String,
    body: String,
}

#[derive(Clone, PartialEq)]
struct Run {
    sender_did: String,
    name: String,
    texts: Vec<String>,
    ts: i64,
}

#[derive(Clone, PartialEq)]
struct Presence {
    did: String,
    name: String,
    typing: bool,
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

fn my_did() -> String {
    hey_core::session::current().map(|s| s.did_key).unwrap_or_default()
}

fn topic(id: &str) -> String {
    format!("hyper-ws/{id}")
}

async fn load_spaces() -> Vec<Space> {
    match storage::read_json("workspaces/index.json").await {
        Ok(Some(v)) => v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        Some(Space {
                            id: x.get("id")?.as_str()?.to_string(),
                            name: x.get("name")?.as_str()?.to_string(),
                            gid: x.get("gid").and_then(Value::as_str).unwrap_or("").to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn save_spaces(list: &[Space]) {
    let v = json!(list
        .iter()
        .map(|s| json!({"id":s.id,"name":s.name,"gid":s.gid}))
        .collect::<Vec<_>>());
    let _ = storage::write_json("workspaces/index.json", &v).await;
}

async fn load_docs(space: &str) -> Vec<Doc> {
    match storage::read_json(&format!("workspaces/{space}/docs.json")).await {
        Ok(Some(v)) => v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        Some(Doc {
                            id: x.get("id")?.as_str()?.to_string(),
                            title: x.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_string(),
                            body: x.get("body").and_then(Value::as_str).unwrap_or("").to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn save_docs(space: &str, docs: &[Doc]) {
    let v = json!(docs
        .iter()
        .map(|d| json!({"id":d.id,"title":d.title,"body":d.body}))
        .collect::<Vec<_>>());
    let _ = storage::write_json(&format!("workspaces/{space}/docs.json"), &v).await;
}

fn group_runs(msgs: &[dms::DmMessage]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for m in msgs {
        if m.text.starts_with('\u{1}') {
            continue;
        }
        let did = if m.sender_did.is_empty() {
            if m.mine {
                my_did()
            } else {
                format!("unknown-{}", m.id)
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
                last.texts.push(m.text.clone());
                last.ts = m.ts;
                continue;
            }
        }
        runs.push(Run {
            sender_did: did,
            name,
            texts: vec![m.text.clone()],
            ts: m.ts,
        });
    }
    runs
}

#[component]
pub fn Workspace() -> impl IntoView {
    let spaces = RwSignal::new(Vec::<Space>::new());
    let open = RwSignal::new(String::new());
    let docs = RwSignal::new(Vec::<Doc>::new());
    let doc_id = RwSignal::new(String::new());
    let draft = RwSignal::new(String::new());
    let dirty = RwSignal::new(false);
    let files_open = RwSignal::new(prefs::ws_files_open());
    let files_w = RwSignal::new(prefs::ws_files_w().clamp(160.0, 360.0));
    let presence = RwSignal::new(Vec::<Presence>::new());
    let huddle_id = RwSignal::new(String::new());
    let thread = RwSignal::new(Vec::<Run>::new());
    let show_thread = RwSignal::new(false);
    let composer = RwSignal::new(String::new());
    let err = RwSignal::new(String::new());

    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    on_cleanup({
        let a = alive.clone();
        move || a.store(false, std::sync::atomic::Ordering::Relaxed)
    });

    spawn_local(async move {
        spaces.set(load_spaces().await);
        while alive.load(std::sync::atomic::Ordering::Relaxed) {
            let sid = open.get_untracked();
            if !sid.is_empty() {
                let t = topic(&sid);
                let _ = peer::join_topic(&t).await;
                if let Ok(v) = peer::recv(peer::RecvArgs {
                    topic: &t,
                    limit: 24,
                    consumer_id: "hyper-desktop-ws",
                    skip_sender_id: Some(&my_did()),
                })
                .await
                {
                    apply_ws_gossip(presence, huddle_id, docs, &v);
                }
                if let Some(sp) = spaces.get_untracked().into_iter().find(|s| s.id == sid) {
                    if !sp.gid.is_empty() {
                        let ms = dms::read_group_conversation(&sp.gid).await;
                        thread.set(group_runs(&ms));
                        // Huddle exists when not looking: scan gcall start without end.
                        let mut live = String::new();
                        for m in ms.iter().rev().take(40) {
                            if let Some(rest) = m.text.strip_prefix("\u{1}hey-gcall:1:") {
                                if let Ok(p) = serde_json::from_str::<Value>(rest) {
                                    let cid = p.get("call_id").and_then(Value::as_str).unwrap_or("");
                                    match p.get("t").and_then(Value::as_str) {
                                        Some("start") | Some("join") if live.is_empty() => {
                                            live = cid.to_string();
                                        }
                                        Some("end") if cid == live => live.clear(),
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if huddle_id.get_untracked().is_empty() && !live.is_empty() {
                            huddle_id.set(live);
                        }
                    }
                }
            }
            gloo_timers::future::TimeoutFuture::new(POLL_MS).await;
        }
    });

    let pick = move |id: String| {
        open.set(id.clone());
        show_thread.set(false);
        spawn_local(async move {
            let list = load_docs(&id).await;
            docs.set(list.clone());
            if let Some(first) = list.first() {
                doc_id.set(first.id.clone());
                draft.set(first.body.clone());
                dirty.set(false);
            }
        });
    };

    let create_space = move || {
        spawn_local(async move {
            let id = format!("ws-{}", now_ms());
            let mut list = spaces.get_untracked();
            list.push(Space {
                id: id.clone(),
                name: "Together".into(),
                gid: String::new(),
            });
            save_spaces(&list).await;
            spaces.set(list);
            pick(id);
        });
    };

    let save_doc = move || {
        let sid = open.get_untracked();
        let did = doc_id.get_untracked();
        let body = draft.get_untracked();
        if sid.is_empty() || did.is_empty() {
            return;
        }
        dirty.set(false);
        spawn_local(async move {
            let mut list = docs.get_untracked();
            if let Some(d) = list.iter_mut().find(|d| d.id == did) {
                d.body = body.clone();
            }
            save_docs(&sid, &list).await;
            docs.set(list);
            let _ = peer::publish(peer::PublishArgs {
                topic: &topic(&sid),
                message: &json!({"t":"doc","id":did,"body":body,"name":"edit"}).to_string(),
                sender_id: &my_did(),
                ts: now_ms(),
                signature: "",
            })
            .await;
        });
    };

    let new_doc = move || {
        let sid = open.get_untracked();
        if sid.is_empty() {
            return;
        }
        let id = format!("doc-{}", now_ms());
        let mut list = docs.get_untracked();
        list.push(Doc {
            id: id.clone(),
            title: "Untitled".into(),
            body: String::new(),
        });
        docs.set(list.clone());
        doc_id.set(id);
        draft.set(String::new());
        spawn_local(async move {
            save_docs(&sid, &list).await;
        });
    };

    let send_chat = move || {
        let text = composer.get_untracked().trim().to_string();
        let sid = open.get_untracked();
        let Some(sp) = spaces.get_untracked().into_iter().find(|s| s.id == sid) else {
            return;
        };
        if text.is_empty() || sp.gid.is_empty() {
            if sp.gid.is_empty() {
                err.set("Link a group to this workspace to talk.".into());
            }
            return;
        }
        composer.set(String::new());
        spawn_local(async move {
            match dms::send_group_message(&sp.gid, &text).await {
                Ok(_) => {}
                Err(e) => err.set(e),
            }
        });
    };

    view! {
        <section class="plane list-pane">
            <header class="bar">
                <h1>"Workspaces"</h1>
                <div class="spring"></div>
                <button class="icon-btn" title="New" on:click=move |_| create_space()>"+"</button>
            </header>
            <div class="body list">
                <For each=move || spaces.get() key=|s| s.id.clone() let:s>
                    {
                        let id = s.id.clone();
                        let id_a = id.clone();
                        view! {
                            <button class="row-btn" class:active=move || open.get()==id_a on:click=move |_| pick(id.clone())>
                                <span class="avatar">{initial(&s.name)}</span>
                                <span class="who"><b>{s.name.clone()}</b><i>"workspace"</i></span>
                            </button>
                        }
                    }
                </For>
            </div>
        </section>

        <section class="plane" style="flex:1;display:flex;flex-direction:column">
            <header class="bar">
                <Show when=move || show_thread.get() fallback=|| ().into_view()>
                    <button class="btn ghost" on:click=move |_| show_thread.set(false)>"Back to conversation"</button>
                </Show>
                <h1>{move || {
                    let id = open.get();
                    spaces.get().into_iter().find(|s| s.id==id).map(|s| s.name).unwrap_or_else(|| "Workspace".into())
                }}</h1>
                <div class="spring"></div>
                <Show when=move || !huddle_id.get().is_empty() fallback=|| ().into_view()>
                    <span class="chip good">"Huddle live"</span>
                </Show>
                <button class="btn ghost" on:click=move |_| {
                    let open_f = !files_open.get();
                    files_open.set(open_f);
                    prefs::set_ws_files_open(open_f);
                }>"Files"</button>
                <button class="btn ghost" on:click=move |_| show_thread.update(|v| *v = !*v)>"Chat"</button>
            </header>

            <Show when=move || !huddle_id.get().is_empty() fallback=|| ().into_view()>
                <div class="huddle-banner">
                    "A huddle is running even if you are in a doc. Join from Calls — it is the same room."
                </div>
            </Show>

            <div class="ws-cols">
                <Show when=move || files_open.get() fallback=|| ().into_view()>
                    <aside class="ws-files" style=move || format!("width:{}px", files_w.get().clamp(160.0, 360.0))>
                        <div class="bar">
                            <h3>"Files"</h3>
                            <button class="btn ghost" on:click=move |_| new_doc()>"New doc"</button>
                        </div>
                        <For each=move || docs.get() key=|d| d.id.clone() let:d>
                            {
                                let id = d.id.clone();
                                let id_a = id.clone();
                                view! {
                                    <button class="row-btn" class:active=move || doc_id.get()==id_a on:click=move |_| {
                                        doc_id.set(id.clone());
                                        if let Some(doc) = docs.get().into_iter().find(|x| x.id==id) {
                                            draft.set(doc.body);
                                            dirty.set(false);
                                        }
                                    }>{d.title.clone()}</button>
                                }
                            }
                        </For>
                        <input class="field" type="range" min="160" max="360" prop:value=move || files_w.get()
                            on:input=move |e| {
                                if let Ok(v) = event_target_value(&e).parse::<f32>() {
                                    let v = v.clamp(160.0, 360.0);
                                    files_w.set(v);
                                    prefs::set_ws_files_w(v);
                                }
                            }
                        />
                    </aside>
                </Show>

                <div class="ws-doc">
                    <Show when=move || !show_thread.get() fallback=move || view! {
                        <div class="thread">
                            <For each=move || thread.get() key=|r| (r.sender_did.clone(), r.ts) let:r>
                                <div class="run" style=tint_style(&r.sender_did)>
                                    <span class="avatar">{initial(&r.name)}</span>
                                    <div>
                                        <b>{r.name.clone()}</b>
                                        <For each=move || r.texts.clone() key=|t| t.clone() let:t>
                                            <p>{t}</p>
                                        </For>
                                    </div>
                                </div>
                            </For>
                            <footer class="composer">
                                <textarea
                                    placeholder="Message — Enter sends, Shift+Enter is a new line"
                                    prop:value=move || composer.get()
                                    on:input=move |e| composer.set(event_target_value(&e))
                                    on:keydown=move |e| {
                                        if e.key()=="Enter" && !e.shift_key() {
                                            e.prevent_default();
                                            send_chat();
                                        }
                                    }
                                ></textarea>
                                <button class="send" on:click=move |_| send_chat()>"\u{2191}"</button>
                            </footer>
                        </div>
                    }>
                        <textarea
                            class="doc"
                            placeholder="Write. Newlines stay. Typing is local until you save — presence goes out now."
                            prop:value=move || draft.get()
                            on:input=move |e| {
                                draft.set(event_target_value(&e));
                                dirty.set(true);
                                let sid = open.get_untracked();
                                spawn_local(async move {
                                    if sid.is_empty() { return; }
                                    let _ = peer::publish(peer::PublishArgs {
                                        topic: &topic(&sid),
                                        message: &json!({"t":"typing","name":"you"}).to_string(),
                                        sender_id: &my_did(),
                                        ts: now_ms(),
                                        signature: "",
                                    }).await;
                                });
                            }
                        ></textarea>
                        <div class="bar">
                            <span class="sub">{move || {
                                let typing = presence.get().into_iter().filter(|p| p.typing).map(|p| p.name).collect::<Vec<_>>();
                                if typing.is_empty() {
                                    if dirty.get() { "Unsaved — save to share.".into() } else { "Saved.".into() }
                                } else {
                                    format!("{} typing…", typing.join(", "))
                                }
                            }}</span>
                            <div class="spring"></div>
                            <button class="btn primary" on:click=move |_| save_doc()>"Save"</button>
                        </div>
                    </Show>
                </div>
            </div>
            <Show when=move || !err.get().is_empty() fallback=|| ().into_view()>
                <p class="note">{move || err.get()}</p>
            </Show>
        </section>
    }
}

fn apply_ws_gossip(presence: RwSignal<Vec<Presence>>, huddle: RwSignal<String>, docs: RwSignal<Vec<Doc>>, v: &Value) {
    let arr = v.get("messages").or_else(|| v.get("items")).and_then(Value::as_array).cloned().unwrap_or_default();
    for m in arr {
        let sender = m.get("sender_id").or_else(|| m.get("sender")).and_then(Value::as_str).unwrap_or("").to_string();
        let raw = m.get("message").or_else(|| m.get("body")).and_then(Value::as_str).unwrap_or("");
        let Ok(j) = serde_json::from_str::<Value>(raw) else { continue };
        match j.get("t").and_then(Value::as_str) {
            Some("typing") => {
                let name = j.get("name").and_then(Value::as_str).unwrap_or(&crate::shorten_did(&sender)).to_string();
                presence.update(|p| {
                    if let Some(row) = p.iter_mut().find(|x| x.did == sender) {
                        row.typing = true;
                    } else {
                        p.push(Presence { did: sender.clone(), name, typing: true });
                    }
                });
            }
            Some("doc") => {
                if let (Some(id), Some(body)) = (j.get("id").and_then(Value::as_str), j.get("body").and_then(Value::as_str)) {
                    docs.update(|list| {
                        if let Some(d) = list.iter_mut().find(|d| d.id == id) {
                            d.body = body.to_string();
                        }
                    });
                }
            }
            Some("huddle") => {
                if let Some(id) = j.get("call_id").and_then(Value::as_str) {
                    huddle.set(id.to_string());
                }
            }
            _ => {}
        }
    }
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

fn tint_style(did: &str) -> String {
    let h = crate::avatar_hue(did);
    format!("--tint:hsl({h} 42% 38%)")
}

// Keep the calls module "used" so a workspace huddle can hand off later
// without a second network. The banner points at Calls on purpose.
#[allow(dead_code)]
fn _handoff() -> Option<calls::Phase> {
    None
}
