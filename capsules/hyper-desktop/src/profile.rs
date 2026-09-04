//! You — one card for the person, then what the network knows about them.
//!
//! The desktop learned this the hard way: it used to present the identity as
//! three option rows in a settings list ("Display name", "Bio", "Edit
//! profile"), which is how you present a preference, not how you present a
//! person. One card, with the picture as the control that changes the picture.
//!
//! Editing writes through `hey_social::api::profile`, the same store hey-social
//! reads, so a name set here is the name that shows there. The DID is minted
//! in this capsule after ElastOS Home authenticates the launch.

use hey_social::api::profile;
use leptos::callback::Callback;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::share::{copy_text, FollowSheet, InviteSheet, LinkDeviceSheet};

#[derive(Clone, PartialEq, Default)]
struct Me {
    name: String,
    bio: String,
    did: String,
    following: usize,
    followers: usize,
    loaded: bool,
}

#[component]
pub fn Profile() -> impl IntoView {
    let (me, set_me) = signal(Me::default());
    let (editing, set_editing) = signal(false);
    let name_draft = RwSignal::new(String::new());
    let bio_draft = RwSignal::new(String::new());
    let invite_open = RwSignal::new(false);
    let follow_open = RwSignal::new(false);
    let link_open = RwSignal::new(false);
    let friend_link = RwSignal::new(String::new());
    let copied = RwSignal::new(false);

    spawn_local(async move {
        let p = profile::read_profile().await.ok().flatten();
        let (following, followers) = profile::follow_counts().await;
        let m = Me {
            name: p.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
            bio: p.as_ref().map(|p| p.bio.clone()).unwrap_or_default(),
            did: p.as_ref().map(|p| p.did_key.clone()).unwrap_or_default(),
            following,
            followers,
            loaded: true,
        };
        name_draft.set(m.name.clone());
        bio_draft.set(m.bio.clone());
        set_me.set(m);
    });

    let save = move || {
        let name = name_draft.get_untracked().trim().to_string();
        let bio = bio_draft.get_untracked().trim().to_string();
        if name.is_empty() {
            return;
        }
        // OPTIMISTIC. The name and the bio are yours; there is nothing for the
        // network to agree to, and waiting on a round trip to see your own
        // typing is what made this feel slow on the desktop. The card updates
        // and the editor closes now; the write goes out behind it.
        set_me.update(|m| {
            m.name = name.clone();
            m.bio = bio.clone();
        });
        set_editing.set(false);
        spawn_local(async move {
            let up = profile::ProfileUpdate {
                name: Some(name),
                bio: Some(bio),
                // NONE, not Some(""). The update is a patch, and sending an
                // empty avatar would erase the picture — which is exactly the
                // data-loss bug the desktop's edit sheet shipped with.
                avatar: None,
            };
            if let Err(e) = profile::update_profile(up).await {
                leptos::logging::warn!("profile save failed: {e:?}");
            }
        });
    };

    view! {
        <section class="plane" style="flex:1">
            <header class="bar">
                <h1>"You"</h1>
                <div class="spring"></div>
            </header>
            <div class="body">
                <div class="card me">
                    <span class="avatar big">
                        {move || initial(&me.get().name)}
                    </span>
                    <Show
                        when=move || !editing.get()
                        fallback=move || {
                            view! {
                                <input
                                    class="field"
                                    placeholder="Display name"
                                    prop:value=move || name_draft.get()
                                    on:input=move |e| name_draft.set(event_target_value(&e))
                                />
                                <input
                                    class="field"
                                    placeholder="Bio"
                                    prop:value=move || bio_draft.get()
                                    on:input=move |e| bio_draft.set(event_target_value(&e))
                                />
                                <div class="btn-row">
                                    <button
                                        class="btn ghost"
                                        on:click=move |_| set_editing.set(false)
                                    >
                                        "Cancel"
                                    </button>
                                    <button class="btn primary" on:click=move |_| save()>
                                        "Save"
                                    </button>
                                </div>
                            }
                        }
                    >
                        <b class="display">
                            {move || {
                                let n = me.get().name;
                                if n.is_empty() { "Not set".to_string() } else { n }
                            }}
                        </b>
                        <span class="key">{move || shorten(&me.get().did)}</span>
                        <span class="bio">{move || me.get().bio}</span>
                        <button class="btn" on:click=move |_| set_editing.set(true)>
                            "Edit profile"
                        </button>
                        <p class="note">
                            "This key is your Hyper account, minted after ElastOS Home launches this capsule."
                        </p>
                    </Show>
                </div>

                <div class="card">
                    <div class="row">
                        <span>
                            <b>"Following"</b>
                            <br />
                            <i class="sub">"Their posts land in your feed."</i>
                        </span>
                        <span class="count">{move || me.get().following}</span>
                    </div>
                    <div class="row" style="margin-top:var(--sp-m)">
                        <span>
                            <b>"Followers"</b>
                            <br />
                            <i class="sub">"They receive yours. Not mutual by default."</i>
                        </span>
                        <span class="count good">{move || me.get().followers}</span>
                    </div>
                </div>

                <div class="card">
                    <h2>"Share"</h2>
                    <p class="note">
                        "Chat invite is one person, sealed. Follow link is your public identity plus a Carrier ticket so their feed can find you. Link a device shares this Home session with another screen, the same job as Skia's code."
                    </p>
                    <div class="btn-row" style="margin-top:var(--sp-m)">
                        <button class="btn primary" on:click=move |_| invite_open.set(true)>
                            "Chat invite"
                        </button>
                        <button class="btn" on:click=move |_| follow_open.set(true)>
                            "Follow someone"
                        </button>
                        <button class="btn" on:click=move |_| link_open.set(true)>
                            "Link a device"
                        </button>
                    </div>
                    <button class="btn ghost" style="margin-top:var(--sp-s)" on:click=move |_| {
                        spawn_local(async move {
                            match profile::my_friend_link().await {
                                Ok(l) => friend_link.set(l),
                                Err(e) => leptos::logging::warn!("friend link: {e:?}"),
                            }
                        });
                    }>"Show my follow link"</button>
                    <Show when=move || !friend_link.get().is_empty() fallback=|| ().into_view()>
                        {move || {
                            let l = friend_link.get();
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
                </div>

                <Appearance />

                <div class="card">
                    <h2>"Home authenticates. This capsule holds the keys."</h2>
                    <p>
                        "ElastOS Home is the login. Ed25519 and ML-KEM live in this capsule. The mesh is ElastOS Carrier."
                    </p>
                </div>
            </div>
        </section>
        <InviteSheet open=invite_open on_joined=Callback::new(move |_did: String| {}) />
        <FollowSheet open=follow_open />
        <LinkDeviceSheet open=link_open />
    }
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

const ACCENTS: [(&str, &str, &str); 5] = [
    ("gold", "Gold", "#e7b85a"),
    ("champagne", "Champagne", "#e8a8a0"),
    ("sky", "Sky", "#7cb8e8"),
    ("mono", "Mono", "#c6cedc"),
    ("violet", "Violet", "#a89cee"),
];

#[component]
fn Appearance() -> impl IntoView {
    let theme = RwSignal::new(crate::prefs::light());
    let accent = RwSignal::new({
        let a = crate::prefs::accent();
        if a.is_empty() { "gold".into() } else { a }
    });
    let rail = expect_context::<RwSignal<bool>>();

    view! {
        <div class="card">
            <h2>"Appearance"</h2>
            <p class="note">"Survives a restart, same as the desktop."</p>
            <div class="btn-row" style="margin-top:var(--sp-m)">
                <button
                    class="btn"
                    class:primary=move || theme.get() == Some(false)
                    on:click=move |_| {
                        crate::prefs::set_light(false);
                        theme.set(Some(false));
                    }
                >
                    "Dark"
                </button>
                <button
                    class="btn"
                    class:primary=move || theme.get() == Some(true)
                    on:click=move |_| {
                        crate::prefs::set_light(true);
                        theme.set(Some(true));
                    }
                >
                    "Light"
                </button>
            </div>
            <div class="swatches">
                {ACCENTS
                    .into_iter()
                    .map(|(key, label, color)| {
                        view! {
                            <button
                                class="swatch"
                                title=label
                                style=format!("background:{color}")
                                aria-current=move || {
                                    if accent.get() == key { "true" } else { "false" }
                                }
                                on:click=move |_| {
                                    crate::prefs::set_accent(key);
                                    accent.set(key.to_string());
                                }
                            ></button>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="row" style="margin-top:var(--sp-l)">
                <span>
                    <b>"Network rail"</b>
                    <br />
                    <i class="sub">"The right-hand pane on Messages, Social and You."</i>
                </span>
                <button
                    class="btn ghost"
                    on:click=move |_| {
                        let on = !rail.get();
                        crate::prefs::set_rail_pinned(on);
                        rail.set(on);
                    }
                >
                    {move || if rail.get() { "Hide" } else { "Show" }}
                </button>
            </div>
        </div>
    }
}

/// The key, shortened for reading. Chars and not bytes — see `chat::short`.
fn shorten(did: &str) -> String {
    if did.is_empty() {
        return "-".into();
    }
    let s = did.strip_prefix("did:key:").unwrap_or(did);
    let n = s.chars().count();
    if n <= 18 {
        return s.to_string();
    }
    let head: String = s.chars().take(10).collect();
    let tail: String = s.chars().skip(n - 6).collect();
    format!("{head}\u{2026}{tail}")
}
