//! Calls — 1:1 and huddles.
//!
//! Signalling rides the existing DM / group-DM control prefixes (`hey-call`,
//! `hey-gcall`) so a phone and this capsule can ring each other. Presence,
//! mute and live tiles go on a Carrier topic named after the call id.
//! Media capture is getUserMedia. Frames are WebCodecs H.264 when the
//! browser has it, JPEG stills otherwise. We never open WebRTC, iroh or UDP.

use crate::media;
use crate::prefs;
use hey_core::api::dms;
use hey_core::runtime::peer;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};
use wasm_bindgen::JsCast;

const CALL_PREFIX: &str = "\u{1}hey-call:1:";
const GCALL_PREFIX: &str = "\u{1}hey-gcall:1:";
const POLL_MS: u32 = 800;
const FRAME_MS: u32 = 220;
const RING_WINDOW_MS: i64 = 63_000;

#[derive(Clone, PartialEq)]
pub struct Contact {
    pub did: String,
    pub name: String,
}

#[derive(Clone, PartialEq)]
pub struct RosterEntry {
    pub did: String,
    pub name: String,
    pub muted: bool,
    pub cam_off: bool,
    pub frame: Option<String>,
    pub decode_fail: bool,
}

#[derive(Clone, PartialEq)]
pub enum Phase {
    Idle,
    Outgoing {
        peer: String,
        name: String,
        video: bool,
        call_id: String,
        route: bool,
    },
    Incoming {
        peer: String,
        name: String,
        video: bool,
        call_id: String,
    },
    Live {
        peer: String,
        name: String,
        video: bool,
        call_id: String,
        huddle: bool,
        gid: String,
    },
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Idle
    }
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

fn my_did() -> String {
    hey_core::session::current()
        .map(|s| s.did_key)
        .unwrap_or_default()
}

fn my_name() -> String {
    hey_core::session::current()
        .map(|s| s.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| crate::shorten_did(&my_did()))
}

fn new_id(prefix: &str) -> String {
    let mut buf = [0u8; 8];
    if let Some(c) = web_sys::window().and_then(|w| w.crypto().ok()) {
        let _ = c.get_random_values_with_u8_array(&mut buf);
    }
    format!("{prefix}-{}-{}", now_ms(), hex8(&buf))
}

fn hex8(b: &[u8]) -> String {
    b.iter().take(8).map(|x| format!("{x:02x}")).collect()
}

fn new_secret() -> String {
    let mut buf = [0u8; 32];
    if let Some(c) = web_sys::window().and_then(|w| w.crypto().ok()) {
        let _ = c.get_random_values_with_u8_array(&mut buf);
    }
    hex8(&buf) + &hex8(&buf[8..])
}

fn call_topic(call_id: &str) -> String {
    format!("hyper-call/{call_id}")
}

async fn send_signal(to: &str, kind: &str, video: bool, call_id: &str, secret: Option<&str>) -> bool {
    let mut obj = json!({ "type": kind, "call_id": call_id });
    if video {
        obj["video"] = Value::Bool(true);
    }
    if let Some(s) = secret {
        obj["secret"] = Value::String(s.to_string());
    }
    let wire = format!("{CALL_PREFIX}{}", obj);
    dms::send_message(to, &wire).await.is_ok()
}

async fn send_gcall(gid: &str, payload: Value) -> bool {
    let wire = format!("{GCALL_PREFIX}{}", payload);
    dms::send_group_message(gid, &wire).await.is_ok()
}

fn parse_call_text(text: &str) -> Option<Value> {
    let rest = text.strip_prefix(CALL_PREFIX)?;
    serde_json::from_str(rest).ok()
}

fn parse_gcall_text(text: &str) -> Option<Value> {
    let rest = text.strip_prefix(GCALL_PREFIX)?;
    serde_json::from_str(rest).ok()
}

async fn gossip(topic: &str, message: &str) {
    let me = my_did();
    let _ = peer::join_topic(topic).await;
    let _ = peer::publish(peer::PublishArgs {
        topic,
        message,
        sender_id: &me,
        ts: now_ms(),
        signature: "",
    })
    .await;
}

/// Publish one call wire. Oversized frames are split with `hey_core::api::frag`
/// so they fit the 4 KB gossip cap. A wire over the 2 MB provider body or the
/// frag reassembly ceiling is dropped (the encoder demotes) — no second store.
async fn gossip_wire(topic: &str, message: &str) {
    if media::wire_too_big(message) {
        return;
    }
    for part in hey_core::api::frag::fragment(message) {
        if part.len() > 2 * 1024 * 1024 {
            return;
        }
        gossip(topic, &part).await;
    }
}

async fn drain_topic(topic: &str) -> Vec<(String, Value)> {
    let me = my_did();
    let v = peer::recv(peer::RecvArgs {
        topic,
        limit: 32,
        consumer_id: "hyper-desktop-calls",
        skip_sender_id: Some(&me),
    })
    .await
    .ok();
    let mut out = Vec::new();
    let Some(v) = v else { return out };
    let arr = v
        .get("messages")
        .or_else(|| v.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for m in arr {
        let sender = m
            .get("sender_id")
            .or_else(|| m.get("sender"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let raw = m
            .get("message")
            .or_else(|| m.get("body"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(wire) = hey_core::api::frag::reassemble(raw) else {
            continue;
        };
        if let Ok(j) = serde_json::from_str::<Value>(&wire) {
            out.push((sender, j));
        }
    }
    out
}

/// Scan recent DMs for a live inbound offer we have not answered.
async fn poll_inbound() -> Option<(String, String, bool, String)> {
    let me = my_did();
    let now = now_ms();
    for c in dms::list_contacts().await {
        for m in dms::read_conversation(&c.did).await.into_iter().rev().take(40) {
            if m.mine {
                continue;
            }
            let Some(p) = parse_call_text(&m.text) else { continue };
            let typ = p.get("type").and_then(Value::as_str).unwrap_or("");
            let call_id = p.get("call_id").and_then(Value::as_str).unwrap_or("");
            if call_id.is_empty() {
                continue;
            }
            if now - m.ts > RING_WINDOW_MS {
                continue;
            }
            if typ == "offer" {
                let video = p.get("video").and_then(Value::as_bool).unwrap_or(false);
                let name = if c.name.is_empty() {
                    crate::shorten_did(&c.did)
                } else {
                    c.name.clone()
                };
                let _ = me;
                return Some((c.did.clone(), name, video, call_id.to_string()));
            }
        }
    }
    None
}

async fn peer_accepted(did: &str, call_id: &str) -> Option<bool> {
    for m in dms::read_conversation(did).await.into_iter().rev().take(40) {
        if m.mine {
            continue;
        }
        let Some(p) = parse_call_text(&m.text) else { continue };
        if p.get("call_id").and_then(Value::as_str) != Some(call_id) {
            continue;
        }
        match p.get("type").and_then(Value::as_str) {
            Some("accept") => return Some(true),
            Some("decline") | Some("end") => return Some(false),
            _ => {}
        }
    }
    None
}

#[component]
pub fn Calls() -> impl IntoView {
    let (contacts, set_contacts) = signal(Vec::<Contact>::new());
    let (groups, set_groups) = signal(Vec::<(String, String)>::new());
    let phase = RwSignal::new(Phase::Idle);
    let settings = RwSignal::new(false);
    let devices = RwSignal::new(Vec::<media::Device>::new());
    let muted = RwSignal::new(false);
    let cam_off = RwSignal::new(false);
    let camera_failed = RwSignal::new(false);
    let route_ok = RwSignal::new(false);
    let roster = RwSignal::new(Vec::<RosterEntry>::new());
    let err = RwSignal::new(String::new());
    let local_ready = RwSignal::new(false);

    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    on_cleanup({
        let alive = alive.clone();
        move || alive.store(false, std::sync::atomic::Ordering::Relaxed)
    });

    spawn_local(async move {
        while alive.load(std::sync::atomic::Ordering::Relaxed) {
            let cs = dms::list_contacts().await;
            set_contacts.set(
                cs.iter()
                    .map(|c| Contact {
                        did: c.did.clone(),
                        name: if c.name.is_empty() {
                            crate::shorten_did(&c.did)
                        } else {
                            c.name.clone()
                        },
                    })
                    .collect(),
            );
            let gs = dms::list_groups().await;
            set_groups.set(
                gs.iter()
                    .map(|g| {
                        let name = if g.name.is_empty() {
                            crate::shorten_did(&g.id)
                        } else {
                            g.name.clone()
                        };
                        (g.id.clone(), name)
                    })
                    .collect(),
            );

            match phase.get_untracked() {
                Phase::Idle => {
                    if let Some((did, name, video, call_id)) = poll_inbound().await {
                        phase.set(Phase::Incoming {
                            peer: did,
                            name,
                            video,
                            call_id,
                        });
                    }
                }
                Phase::Outgoing {
                    peer: p,
                    name,
                    video,
                    call_id,
                    ..
                } => {
                    let topic = call_topic(&call_id);
                    let _ = peer::join_topic(&topic).await;
                    let proved = peer::has_topic_peer(&topic).await;
                    route_ok.set(proved);
                    phase.set(Phase::Outgoing {
                        peer: p.clone(),
                        name: name.clone(),
                        video,
                        call_id: call_id.clone(),
                        route: proved,
                    });
                    match peer_accepted(&p, &call_id).await {
                        Some(true) => {
                            // Do not open live media on an unproved route.
                            if proved {
                                phase.set(Phase::Live {
                                    peer: p,
                                    name,
                                    video,
                                    call_id,
                                    huddle: false,
                                    gid: String::new(),
                                });
                            }
                        }
                        Some(false) => {
                            phase.set(Phase::Idle);
                            err.set("They declined.".into());
                        }
                        None => {}
                    }
                }
                Phase::Live {
                    call_id,
                    huddle,
                    gid,
                    peer: p,
                    ..
                } => {
                    let topic = call_topic(&call_id);
                    let _ = peer::join_topic(&topic).await;
                    route_ok.set(peer::has_topic_peer(&topic).await);
                    for (sender, msg) in drain_topic(&topic).await {
                        apply_presence(roster, sender, msg);
                    }
                    if huddle && !gid.is_empty() {
                        for m in dms::read_group_conversation(&gid).await.into_iter().rev().take(30) {
                            if let Some(g) = parse_gcall_text(&m.text) {
                                if g.get("t").and_then(Value::as_str) == Some("end")
                                    && g.get("call_id").and_then(Value::as_str) == Some(call_id.as_str())
                                {
                                    phase.set(Phase::Idle);
                                }
                            }
                        }
                    } else if peer_accepted(&p, &call_id).await == Some(false) {
                        phase.set(Phase::Idle);
                    }
                }
                Phase::Incoming { .. } => {}
            }
            gloo_timers::future::TimeoutFuture::new(POLL_MS).await;
        }
    });

    let refresh_devices = move || {
        spawn_local(async move {
            devices.set(media::enumerate().await);
        });
    };

    let place = move |did: String, name: String, video: bool| {
        if !matches!(phase.get_untracked(), Phase::Idle) {
            return;
        }
        let call_id = new_id("c");
        let secret = new_secret();
        phase.set(Phase::Outgoing {
            peer: did.clone(),
            name,
            video,
            call_id: call_id.clone(),
            route: false,
        });
        spawn_local(async move {
            let topic = call_topic(&call_id);
            let _ = peer::join_topic(&topic).await;
            // Signalling first. Media waits for accept + a proved peer.
            let ok = send_signal(&did, "offer", video, &call_id, Some(&secret)).await;
            if !ok {
                err.set("Could not reach them — the invite did not send.".into());
                phase.set(Phase::Idle);
            }
        });
    };

    let accept = move || {
        let Phase::Incoming {
            peer: p,
            name,
            video,
            call_id,
        } = phase.get_untracked()
        else {
            return;
        };
        spawn_local(async move {
            let topic = call_topic(&call_id);
            let _ = peer::join_topic(&topic).await;
            let proved = peer::wait_for_topic_peers(&topic, &[]).await;
            route_ok.set(proved);
            let _ = send_signal(&p, "accept", video, &call_id, None).await;
            if proved {
                phase.set(Phase::Live {
                    peer: p,
                    name,
                    video,
                    call_id,
                    huddle: false,
                    gid: String::new(),
                });
            } else {
                err.set("Invite accepted, but the Carrier path is not proved yet. Waiting…".into());
                // Still enter live so presence can start once a peer appears;
                // media capture stays gated on route_ok.
                phase.set(Phase::Live {
                    peer: p,
                    name,
                    video,
                    call_id,
                    huddle: false,
                    gid: String::new(),
                });
            }
        });
    };

    let decline = move || {
        if let Phase::Incoming {
            peer: p, call_id, video, ..
        } = phase.get_untracked()
        {
            spawn_local(async move {
                let _ = send_signal(&p, "decline", video, &call_id, None).await;
            });
        }
        phase.set(Phase::Idle);
    };

    let hangup = move || {
        match phase.get_untracked() {
            Phase::Outgoing {
                peer: p,
                call_id,
                video,
                ..
            }
            | Phase::Live {
                peer: p,
                call_id,
                video,
                huddle: false,
                ..
            } => {
                spawn_local(async move {
                    let _ = send_signal(&p, "end", video, &call_id, None).await;
                    let _ = peer::leave_topic(&call_topic(&call_id)).await;
                });
            }
            Phase::Live {
                gid,
                call_id,
                huddle: true,
                ..
            } => {
                spawn_local(async move {
                    let _ = send_gcall(
                        &gid,
                        json!({"t":"end","call_id":call_id,"did":my_did()}),
                    )
                    .await;
                    let _ = peer::leave_topic(&call_topic(&call_id)).await;
                });
            }
            _ => {}
        }
        local_ready.set(false);
        camera_failed.set(false);
        phase.set(Phase::Idle);
        roster.set(Vec::new());
    };

    let start_huddle = move |gid: String, name: String, video: bool| {
        let call_id = new_id("gc");
        phase.set(Phase::Live {
            peer: gid.clone(),
            name,
            video,
            call_id: call_id.clone(),
            huddle: true,
            gid: gid.clone(),
        });
        spawn_local(async move {
            let topic = call_topic(&call_id);
            let _ = peer::join_topic(&topic).await;
            let _ = send_gcall(
                &gid,
                json!({
                    "t":"start",
                    "call_id": call_id,
                    "did": my_did(),
                    "name": my_name(),
                    "video": video,
                    "secret": new_secret(),
                    "mc": true
                }),
            )
            .await;
            let _ = send_gcall(
                &gid,
                json!({"t":"join","call_id":call_id,"did":my_did(),"name":my_name()}),
            )
            .await;
        });
    };

    view! {
        <section class="plane list-pane">
            <header class="bar">
                <h1>"Calls"</h1>
                <div class="spring"></div>
                <button class="btn ghost" on:click=move |_| { settings.set(true); refresh_devices(); }>
                    "Settings"
                </button>
            </header>
            <div class="body list">
                <p class="empty" style="padding-bottom:8px">
                    "1:1 rings a contact. A huddle is a group call. Settings work without being in a call."
                </p>
                <For
                    each=move || contacts.get()
                    key=|c| c.did.clone()
                    let:c
                >
                    {
                        let did = c.did.clone();
                        let name = c.name.clone();
                        let did2 = did.clone();
                        let name2 = name.clone();
                        view! {
                            <div class="row">
                                <span class="avatar" style=tint_style(&did)>{initial(&name)}</span>
                                <span class="who"><b>{name.clone()}</b><i>{crate::shorten_did(&did)}</i></span>
                                <button class="icon-btn" title="Voice" on:click=move |_| place(did.clone(), name.clone(), false)>"\u{1F3A4}"</button>
                                <button class="icon-btn" title="Video" on:click=move |_| place(did2.clone(), name2.clone(), true)>"\u{1F3A5}"</button>
                            </div>
                        }
                    }
                </For>
                <h3 style="margin:16px 12px 8px">"Huddles"</h3>
                <For
                    each=move || groups.get()
                    key=|g| g.0.clone()
                    let:g
                >
                    {
                        let gid = g.0.clone();
                        let name = g.1.clone();
                        let gid2 = gid.clone();
                        let name2 = name.clone();
                        view! {
                            <div class="row">
                                <span class="avatar">{initial(&name)}</span>
                                <span class="who"><b>{name.clone()}</b><i>"group"</i></span>
                                <button class="btn ghost" on:click=move |_| start_huddle(gid.clone(), name.clone(), false)>"Huddle"</button>
                                <button class="btn ghost" on:click=move |_| start_huddle(gid2.clone(), name2.clone(), true)>"Video"</button>
                            </div>
                        }
                    }
                </For>
            </div>
        </section>

        <section class="plane" style="flex:1">
            <header class="bar">
                <h1>{move || match phase.get() {
                    Phase::Idle => "No call".into(),
                    Phase::Outgoing { name, .. } => format!("Calling {name}"),
                    Phase::Incoming { name, .. } => format!("{name} is calling"),
                    Phase::Live { name, huddle, .. } => if huddle { format!("Huddle · {name}") } else { name },
                }}</h1>
                <div class="spring"></div>
                <Show when=move || route_ok.get() fallback=|| view!{<span class="chip">"route unproved"</span>}>
                    <span class="chip good">"Carrier path"</span>
                </Show>
            </header>
            <div class="body call-stage">
                {move || match phase.get() {
                    Phase::Idle => view! {
                        <div class="card">
                            <h2>"Pick someone to call"</h2>
                            <p>"Signalling uses the same sealed DM the phone does. Live tiles wait until Carrier has a peer on the call topic — we will not claim a camera is live on a dead path."</p>
                        </div>
                    }.into_any(),
                    Phase::Outgoing { name, video, route, .. } => view! {
                        <div class="card">
                            <h2>{format!("Ringing {name}")}</h2>
                            <p>{if video { "Video invite sent." } else { "Voice invite sent." }}</p>
                            <p>{if route { "Path is up. Waiting for them to pick up." } else { "Waiting for a proved Carrier route before live media." }}</p>
                            <button class="btn" on:click=move |_| hangup()>"Cancel"</button>
                        </div>
                    }.into_any(),
                    Phase::Incoming { name, video, .. } => view! {
                        <div class="card">
                            <h2>{format!("{name} is calling")}</h2>
                            <p>{if video { "Video" } else { "Voice" }}</p>
                            <div class="btn-row">
                                <button class="btn primary" on:click=move |_| accept()>"Accept"</button>
                                <button class="btn ghost" on:click=move |_| decline()>"Decline"</button>
                            </div>
                        </div>
                    }.into_any(),
                    Phase::Live { name, video, call_id, huddle, .. } => view! {
                        <LiveStage
                            name=name
                            video=video
                            call_id=call_id
                            huddle=huddle
                            muted=muted
                            cam_off=cam_off
                            camera_failed=camera_failed
                            route_ok=route_ok
                            roster=roster
                            local_ready=local_ready
                            hangup=hangup
                            open_settings=move || { settings.set(true); refresh_devices(); }
                        />
                    }.into_any(),
                }}
                <Show when=move || !err.get().is_empty() fallback=|| ().into_view()>
                    <div class="card"><p>{move || err.get()}</p></div>
                </Show>
            </div>
        </section>

        <Show when=move || settings.get() fallback=|| ().into_view()>
            <SettingsSheet
                devices=devices
                close=move || settings.set(false)
                refresh=refresh_devices
            />
        </Show>
    }
}

fn upsert_tile(rows: &mut Vec<RosterEntry>, sender: &str, name: &str, frame: Option<String>, decode_fail: bool) {
    if let Some(row) = rows.iter_mut().find(|r| r.did == sender) {
        if frame.is_some() {
            row.frame = frame;
        }
        row.decode_fail = decode_fail;
        if !name.is_empty() {
            row.name = name.to_string();
        }
    } else {
        rows.push(RosterEntry {
            did: sender.to_string(),
            name: if name.is_empty() { crate::shorten_did(sender) } else { name.to_string() },
            muted: false,
            cam_off: false,
            frame,
            decode_fail,
        });
    }
}

fn apply_presence(roster: RwSignal<Vec<RosterEntry>>, sender: String, msg: Value) {
    if sender.is_empty() {
        return;
    }
    let kind = msg.get("t").and_then(Value::as_str).unwrap_or("");
    roster.update(|rows| {
        let i = rows.iter().position(|r| r.did == sender);
        let mut row = i.map(|i| rows[i].clone()).unwrap_or(RosterEntry {
            did: sender.clone(),
            name: msg.get("name").and_then(Value::as_str).unwrap_or(&crate::shorten_did(&sender)).to_string(),
            muted: false,
            cam_off: false,
            frame: None,
            decode_fail: false,
        });
        match kind {
            "leave" => {
                rows.retain(|r| r.did != sender);
                return;
            }
            "mute" => row.muted = msg.get("on").and_then(Value::as_bool).unwrap_or(true),
            "cam" => row.cam_off = msg.get("off").and_then(Value::as_bool).unwrap_or(true),
            "frame" => {
                if let Some(avc) = msg.get("avc").and_then(Value::as_str) {
                    let key = msg.get("key").and_then(Value::as_bool).unwrap_or(false);
                    let pts = msg.get("pts").and_then(Value::as_i64).unwrap_or(0);
                    let avc = avc.to_string();
                    let sender = sender.clone();
                    let name = row.name.clone();
                    spawn_local(async move {
                        match media::decode_avc(&sender, &avc, key, pts).await {
                            Ok(url) => roster.update(|rows| {
                                upsert_tile(rows, &sender, &name, Some(url), false);
                            }),
                            Err(e) => {
                                leptos::logging::warn!("avc decode: {e}");
                                roster.update(|rows| {
                                    upsert_tile(rows, &sender, &name, None, true);
                                });
                            }
                        }
                    });
                    return;
                }
                if let Some(url) = msg.get("jpeg").and_then(Value::as_str) {
                    if url.starts_with("data:image/") {
                        row.frame = Some(url.to_string());
                        row.decode_fail = false;
                    } else {
                        row.decode_fail = true;
                    }
                } else {
                    row.decode_fail = true;
                }
            }
            _ => {}
        }
        if let Some(i) = rows.iter().position(|r| r.did == sender) {
            rows[i] = row;
        } else {
            rows.push(row);
        }
    });
}

#[component]
fn LiveStage(
    name: String,
    video: bool,
    call_id: String,
    huddle: bool,
    muted: RwSignal<bool>,
    cam_off: RwSignal<bool>,
    camera_failed: RwSignal<bool>,
    route_ok: RwSignal<bool>,
    roster: RwSignal<Vec<RosterEntry>>,
    local_ready: RwSignal<bool>,
    hangup: impl Fn() + Copy + 'static,
    open_settings: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();

    Effect::new({
        let call_id = call_id.clone();
        move |_| {
            let call_id = call_id.clone();
            let want_video = video && !cam_off.get();
            let route = route_ok.get();
            spawn_local(async move {
                if !route {
                    local_ready.set(false);
                    return;
                }
                match media::open_stream(want_video, true).await {
                    Ok(stream) => {
                        camera_failed.set(false);
                        if let Some(el) = video_ref.get() {
                            if let Ok(el) = el.dyn_into::<web_sys::HtmlVideoElement>() {
                                media::attach_local(&el, &stream);
                            }
                        }
                        local_ready.set(true);
                        let topic = call_topic(&call_id);
                        let _ = gossip(
                            &topic,
                            &json!({"t":"join","did":my_did(),"name":my_name()}).to_string(),
                        )
                        .await;
                    }
                    Err(e) => {
                        // Failed, not off. The self-view must not look like a
                        // user who turned their camera off.
                        camera_failed.set(true);
                        local_ready.set(false);
                        leptos::logging::warn!("camera: {e}");
                    }
                }
            });
        }
    });

    // H.264 (WebCodecs) or JPEG ladder on a proved route only.
    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    on_cleanup({
        let alive = alive.clone();
        move || {
            alive.store(false, std::sync::atomic::Ordering::Relaxed);
            media::reset_codecs();
        }
    });
    spawn_local({
        let call_id = call_id.clone();
        async move {
            while alive.load(std::sync::atomic::Ordering::Relaxed) {
                gloo_timers::future::TimeoutFuture::new(FRAME_MS).await;
                if !route_ok.get_untracked() || cam_off.get_untracked() || !video {
                    continue;
                }
                let Some(el) = video_ref.get_untracked() else { continue };
                let Ok(el) = el.dyn_into::<web_sys::HtmlVideoElement>() else { continue };
                let room = roster.get_untracked().len() + 1;
                let direct = route_ok.get_untracked() && !huddle;
                let topic = call_topic(&call_id);
                if let Some(nal) = media::encode_avc(&el, room, direct).await {
                    let wire = json!({
                        "t": "frame",
                        "avc": media::b64_encode(&nal.bytes),
                        "key": nal.key,
                        "pts": nal.pts,
                        "name": my_name(),
                    })
                    .to_string();
                    gossip_wire(&topic, &wire).await;
                } else if let Some(jpeg) = media::snapshot_jpeg(&el, room, direct) {
                    let wire = json!({"t":"frame","jpeg":jpeg,"name":my_name()}).to_string();
                    gossip_wire(&topic, &wire).await;
                }
            }
        }
    });

    view! {
        <div class="stage">
            <div class="tiles">
                <div class="tile me">
                    <video node_ref=video_ref autoplay playsinline muted></video>
                    {move || {
                        if camera_failed.get() && video {
                            view!{<div class="tile-label warn">"No camera — pick one in Call settings"</div>}.into_any()
                        } else if cam_off.get() && video {
                            view!{<div class="tile-label">"camera off"</div>}.into_any()
                        } else if !local_ready.get() && video {
                            view!{<div class="tile-label">"waiting for a proved route…"</div>}.into_any()
                        } else {
                            view!{<div class="tile-label">{format!("You · {name}")}</div>}.into_any()
                        }
                    }}
                </div>
                <For each=move || roster.get() key=|r| r.did.clone() let:r>
                    {
                        let label = format!("{}{}", r.name, if r.muted { " · muted" } else { "" });
                        let r2 = r.clone();
                        view! {
                    <div class="tile" style=tint_style(&r.did)>
                        {move || {
                            if r2.decode_fail {
                                view!{<div class="tile-label warn">"Remote frame would not decode"</div>}.into_any()
                            } else if let Some(url) = r2.frame.clone() {
                                view!{<img src=url />}.into_any()
                            } else if r2.cam_off {
                                view!{<div class="tile-label">"camera off"</div>}.into_any()
                            } else {
                                view!{<div class="avatar big">{initial(&r2.name)}</div>}.into_any()
                            }
                        }}
                        <div class="tile-label">{label}</div>
                    </div>
                        }
                    }
                </For>
            </div>
            <div class="call-bar">
                {
                    let cid_mute = call_id.clone();
                    let cid_cam = call_id.clone();
                    view! {
                <button class="btn" class:primary=move || muted.get() on:click=move |_| {
                    muted.update(|m| *m = !*m);
                    let on = muted.get();
                    let cid = cid_mute.clone();
                    spawn_local(async move {
                        let _ = gossip(&call_topic(&cid), &json!({"t":"mute","on":on}).to_string()).await;
                    });
                }>{move || if muted.get() { "Unmute" } else { "Mute" }}</button>
                <button class="btn" on:click=move |_| {
                    cam_off.update(|c| *c = !*c);
                    let off = cam_off.get();
                    let cid = cid_cam.clone();
                    spawn_local(async move {
                        let _ = gossip(&call_topic(&cid), &json!({"t":"cam","off":off}).to_string()).await;
                    });
                }>{move || if cam_off.get() { "Camera on" } else { "Camera off" }}</button>
                    }
                }
                <button class="btn ghost" on:click=move |_| open_settings()>"Settings"</button>
                <button class="btn" on:click=move |_| hangup()>"Leave"</button>
            </div>
        </div>
    }
}

#[component]
fn SettingsSheet(
    devices: RwSignal<Vec<media::Device>>,
    close: impl Fn() + Copy + 'static,
    refresh: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let cam = RwSignal::new(prefs::call_camera());
    let mic = RwSignal::new(prefs::call_mic());
    let spk = RwSignal::new(prefs::call_speaker());
    refresh();
    view! {
        <div class="sheet-wrap" on:click=move |_| close()>
            <div class="sheet" on:click=move |e| e.stop_propagation()>
                <header class="bar">
                    <h1>"Call settings"</h1>
                    <div class="spring"></div>
                    <button class="btn ghost" on:click=move |_| close()>"Close"</button>
                </header>
                <div class="sheet-scroll">
                    <p class="note">"Reachable without a live call. Devices are stored by name, so two cameras that share a label stay distinguishable and unplugging one cannot silently retarget the other."</p>
                    <h3>"Camera"</h3>
                    <For each={move || devices.get().into_iter().filter(|d| d.kind=="camera").collect::<Vec<_>>()} key=|d| d.id.clone() let:d>
                        {
                            let label = d.label.clone();
                            let selected = {
                                let l = label.clone();
                                move || cam.get() == l
                            };
                            view! {
                                <button class="row-btn" class:active=selected on:click=move |_| {
                                    cam.set(label.clone());
                                    prefs::set_call_camera(&label);
                                }>{d.label.clone()}</button>
                            }
                        }
                    </For>
                    <h3>"Microphone"</h3>
                    <For each={move || devices.get().into_iter().filter(|d| d.kind=="mic").collect::<Vec<_>>()} key=|d| d.id.clone() let:d>
                        {
                            let label = d.label.clone();
                            let label_a = label.clone();
                            view! {
                                <button class="row-btn" class:active=move || mic.get() == label_a on:click=move |_| {
                                    mic.set(label.clone());
                                    prefs::set_call_mic(&label);
                                }>{d.label.clone()}</button>
                            }
                        }
                    </For>
                    <h3>"Speaker"</h3>
                    <For each={move || devices.get().into_iter().filter(|d| d.kind=="speaker").collect::<Vec<_>>()} key=|d| d.id.clone() let:d>
                        {
                            let label = d.label.clone();
                            let label_a = label.clone();
                            view! {
                                <button class="row-btn" class:active=move || spk.get() == label_a on:click=move |_| {
                                    spk.set(label.clone());
                                    prefs::set_call_speaker(&label);
                                }>{d.label.clone()}</button>
                            }
                        }
                    </For>
                    <p class="note">"Labels stay empty until the browser has been granted capture once. Accept a call or tap a device after permission."</p>
                </div>
            </div>
        </div>
    }
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

fn tint_style(did: &str) -> String {
    let h = crate::avatar_hue(did);
    format!("--tint:hsl({h} 42% 38%)")
}
