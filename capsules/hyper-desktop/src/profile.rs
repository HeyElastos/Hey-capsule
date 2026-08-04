//! You — one card for the person, then what the network knows about them.
//!
//! The desktop learned this the hard way: it used to present the identity as
//! three option rows in a settings list ("Display name", "Bio", "Edit
//! profile"), which is how you present a preference, not how you present a
//! person. One card, with the picture as the control that changes the picture.
//!
//! Editing writes through `hey_social::api::profile`, the same store hey-social
//! reads, so a name set here is the name that shows there. The DID is NOT
//! editable and is not ours to set — it comes from ElastOS.

use hey_social::api::profile;
use leptos::prelude::*;
use leptos::task::spawn_local;

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
                            "This key is your account. ElastOS holds it \u{2014} this app never sees it."
                        </p>
                    </Show>
                </div>

                <div class="card">
                    <div class="row">
                        <span>
                            <b>"Following"</b>
                            <br />
                            <i class="sub">"Their posts arrive in your feed."</i>
                        </span>
                        <span class="count">{move || me.get().following}</span>
                    </div>
                    <div class="row" style="margin-top:var(--sp-m)">
                        <span>
                            <b>"Followers"</b>
                            <br />
                            <i class="sub">"They receive yours. Following is not mutual by default."</i>
                        </span>
                        <span class="count good">{move || me.get().followers}</span>
                    </div>
                </div>

                <div class="card">
                    <h2>"Keys stay with the runtime"</h2>
                    <p>
                        "Signing happens in ElastOS. This capsule asks for a signature and never holds the key, so nothing sensitive lives in the page."
                    </p>
                </div>
            </div>
        </section>
    }
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

/// The key, shortened for reading. Chars and not bytes — see `chat::short`.
fn shorten(did: &str) -> String {
    if did.is_empty() {
        return "\u{2014}".into();
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
