// PostCard — Rust port of capsules/hey-social/client/src/components/PostCard.jsx.
//
// Renders a single post: author header, media carousel (one image for now —
// multi-image carousel is Phase 2), caption, reaction count, comment count.
// Reactions are interactive (tap the heart to toggle); composing comments
// is currently read-only — the comment input is a follow-up.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::posts::{react_to_post, Post};
use crate::components::icons::{CommentIcon, HeartIcon};
use crate::runtime::ipfs;
use crate::session;

#[component]
pub fn PostCard(post: Post) -> impl IntoView {
    let post_signal = RwSignal::new(post);
    let me_did = session::current().map(|s| s.did_key).unwrap_or_default();

    let i_reacted = Memo::new(move |_| {
        let p = post_signal.read();
        p.reactions
            .get("❤️")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|v| v.as_str() == Some(&me_did)))
            .unwrap_or(false)
    });
    let react_count = Memo::new(move |_| {
        let p = post_signal.read();
        p.reactions
            .values()
            .filter_map(|v| v.as_array())
            .map(|a| a.len())
            .sum::<usize>()
    });
    let comment_count = Memo::new(move |_| post_signal.read().comments.len());

    let toggle_react = move |_| {
        let id = post_signal.read().id.clone();
        spawn_local(async move {
            if let Ok(updated) = react_to_post(&id, "❤️").await {
                post_signal.set(updated);
            }
        });
    };

    view! {
        <article class="rounded-2xl overflow-hidden bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 shadow-sm">
            <PostHeader post=post_signal />
            <PostMedia post=post_signal />
            <div class="p-4 space-y-3">
                <div class="flex items-center gap-4">
                    <button
                        type="button"
                        on:click=toggle_react
                        class="inline-flex items-center gap-1.5 text-sm transition-colors"
                        class:text-rose-500=move || i_reacted.get()
                        class:text-slate-500=move || !i_reacted.get()
                    >
                        <HeartIcon class="h-5 w-5" filled=i_reacted.get() />
                        <span>{move || react_count.get()}</span>
                    </button>
                    <span class="inline-flex items-center gap-1.5 text-sm text-slate-500 dark:text-slate-400">
                        <CommentIcon class="h-5 w-5" />
                        <span>{move || comment_count.get()}</span>
                    </span>
                </div>
                <PostCaption post=post_signal />
            </div>
        </article>
    }
}

#[component]
fn PostHeader(post: RwSignal<Post>) -> impl IntoView {
    view! {
        <header class="flex items-center gap-3 p-4">
            <div class="h-10 w-10 rounded-full bg-gradient-to-br from-amber-400 to-rose-400 grid place-items-center text-white text-sm font-semibold">
                {move || initial_letter(&post.read().user_name)}
            </div>
            <div class="min-w-0">
                <p class="text-sm font-medium text-slate-900 dark:text-slate-100 truncate">
                    {move || post.read().user_name.clone()}
                </p>
                <p class="text-[11px] text-slate-500 dark:text-slate-400 font-mono truncate">
                    {move || post.read().user_did.chars().take(20).collect::<String>() + "…"}
                </p>
            </div>
        </header>
    }
}

#[component]
fn PostMedia(post: RwSignal<Post>) -> impl IntoView {
    let media = Memo::new(move |_| post.read().images.first().cloned());
    view! {
        {move || match media.get() {
            Some(m) if m.media_type == "video" => view! {
                <video
                    controls
                    class="block w-full bg-black"
                    src=ipfs::gateway_url(&m.cid, None)
                />
            }.into_any(),
            Some(m) => view! {
                <img
                    class="block w-full bg-slate-100 dark:bg-slate-800 aspect-square object-cover"
                    src=ipfs::gateway_url(&m.cid, None)
                    alt=m.name.clone()
                    loading="lazy"
                />
            }.into_any(),
            None => view! { <></> }.into_any(),
        }}
    }
}

#[component]
fn PostCaption(post: RwSignal<Post>) -> impl IntoView {
    view! {
        {move || {
            let caption = post.read().caption.clone();
            if caption.is_empty() {
                view! { <></> }.into_any()
            } else {
                view! {
                    <p class="text-sm text-slate-700 dark:text-slate-300 leading-snug whitespace-pre-wrap">{caption}</p>
                }.into_any()
            }
        }}
    }
}

fn initial_letter(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().next().unwrap_or(c).to_string())
        .unwrap_or_else(|| "?".into())
}
