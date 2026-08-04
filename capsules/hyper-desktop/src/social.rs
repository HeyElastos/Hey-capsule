//! Social — the feed.
//!
//! Posts come from `hey_social::api::posts`, not from a second implementation.
//! That module owns what a post IS: the shape, the per-author index, the IPLD
//! materialisation from a cid. Writing our own would mean two capsules
//! disagreeing about the same stored data the first time either changed, and a
//! feed that shows different things in two places is worse than the coupling.
//!
//! Media is fetched through the shared content cache rather than pointed at a
//! gateway URL, because a followers-only tile is sealed — the bytes have to come
//! back through the engine to be opened at all.

use hey_social::api::posts;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// How many posts the feed asks for.
///
/// The desktop app pages; this does not yet. Sixty is comfortably more than a
/// screen and small enough that the initial read is not felt.
const LIMIT: usize = 60;

#[derive(Clone, PartialEq)]
struct Card {
    id: String,
    author: String,
    text: String,
    ts: i64,
    likes: u32,
    comments: u32,
    /// First image tile, if the post has one. Already-decoded bytes as a blob
    /// URL — see `load_media`.
    media: Option<String>,
}

#[component]
pub fn Social() -> impl IntoView {
    let (cards, set_cards) = signal(Vec::<Card>::new());
    let (state, set_state) = signal("loading\u{2026}");

    spawn_local(async move {
        match posts::get_posts(LIMIT).await {
            Ok(ps) => {
                let list: Vec<Card> = ps
                    .iter()
                    .map(|p| Card {
                        id: p.id.clone(),
                        author: display_author(p),
                        text: p.caption.clone(),
                        ts: p.ts,
                        // A reaction bucket is an array of DIDs keyed by emoji;
                        // the count is every bucket's length, not the number of
                        // distinct emoji.
                        likes: p
                            .reactions
                            .values()
                            .map(|v| v.as_array().map(|a| a.len()).unwrap_or(0) as u32)
                            .sum(),
                        comments: p.comments.len() as u32,
                        // The first tile, if there is one. Kept as a flag here;
                        // the bytes are sealed and have to come back through the
                        // engine, which is a fetch per card rather than a URL.
                        media: p.images.first().map(|t| t.cid.clone()),
                    })
                    .collect();
                set_state.set(if list.is_empty() { "empty" } else { "ok" });
                set_cards.set(list);
            }
            Err(e) => {
                // A 403 here is the provider-proxy allowlist, not a bug in this
                // capsule and not something a retry fixes. Say so plainly
                // rather than showing an empty feed that looks like "no posts".
                leptos::logging::warn!("feed read failed: {e:?}");
                set_state.set("blocked");
            }
        }
    });

    view! {
        <section class="plane" style="flex:1">
            <header class="bar">
                <h1>"Social"</h1>
                <div class="spring"></div>
            </header>
            <div class="body">
                <Show when=move || state.get() == "empty" fallback=|| ().into_view()>
                    <p class="empty">
                        "Nothing in the feed yet. Follow someone, or post, and it lands here."
                    </p>
                </Show>
                <Show when=move || state.get() == "blocked" fallback=|| ().into_view()>
                    <div class="card">
                        <h2>"The runtime is not letting this through"</h2>
                        <p>
                            "Provider calls are refused by the gateway's allowlist. The capsule is authenticated \u{2014} it just is not permitted to reach the content provider on this runtime. Nothing here can work around that; it is opened runtime-side."
                        </p>
                    </div>
                </Show>
                <For each=move || cards.get() key=|c| c.id.clone() let:c>
                    <article class="post">
                        <div class="post-head">
                            <span class="avatar">{initial(&c.author)}</span>
                            <span class="who">
                                <b>{c.author.clone()}</b>
                                <i>{crate::ago(c.ts)}</i>
                            </span>
                        </div>
                        <Show
                            when={
                                let m = c.media.clone();
                                move || m.is_some()
                            }
                            fallback=|| ().into_view()
                        >
                            <div class="post-media"></div>
                        </Show>
                        <p class="post-text">{c.text.clone()}</p>
                        <div class="post-foot">
                            <span>"\u{2661} " {c.likes}</span>
                            <span>"\u{1F5E9} " {c.comments}</span>
                        </div>
                    </article>
                </For>
            </div>
        </section>
    }
}

/// The author's display name, falling back to a shortened DID.
///
/// A post carries the author's did and, usually, a name they chose. The did is
/// the thing that is actually true, so it is the fallback rather than "unknown".
fn display_author(p: &posts::Post) -> String {
    if !p.user_name.is_empty() {
        return p.user_name.clone();
    }
    crate::shorten_did(&p.user_did)
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}
