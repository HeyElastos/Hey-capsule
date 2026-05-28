// Clips — short-form video feed. Same data source as Home, filtered to
// posts whose first media is a video. Rust port of capsules/hey-social/
// client/src/pages/Clips.jsx.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::posts::{get_posts, Post};
use crate::components::icons::ClipsIcon;
use crate::components::{FloatingDock, PostCard};
use crate::pages::misc::AppShell;

#[component]
pub fn Clips() -> impl IntoView {
    let posts: RwSignal<Vec<Post>> = RwSignal::new(Vec::new());
    let loading = RwSignal::new(true);

    Effect::new(move |_| {
        loading.set(true);
        spawn_local(async move {
            let p = get_posts(50).await.unwrap_or_default();
            posts.set(p);
            loading.set(false);
        });
    });

    let video_posts = Memo::new(move |_| {
        posts
            .read()
            .iter()
            .filter(|p| p.images.first().map(|m| m.media_type == "video").unwrap_or(false))
            .cloned()
            .collect::<Vec<_>>()
    });

    view! {
        <AppShell>
            <div class="mx-auto max-w-2xl px-4 pt-6 pb-28 space-y-6">
                {move || if loading.get() {
                    view! {
                        <div class="rounded-2xl bg-slate-200 dark:bg-slate-800 animate-pulse aspect-video" />
                    }.into_any()
                } else if video_posts.read().is_empty() {
                    view! {
                        <div class="rounded-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 p-10 text-center">
                            <div class="inline-flex h-16 w-16 items-center justify-center rounded-2xl bg-rose-50 dark:bg-rose-500/15 text-rose-600 dark:text-rose-300">
                                <ClipsIcon class="h-7 w-7" />
                            </div>
                            <h2 class="mt-5 text-2xl font-semibold text-slate-900 dark:text-slate-50">
                                "No clips yet"
                            </h2>
                            <p class="mx-auto mt-3 max-w-sm text-sm text-slate-600 dark:text-slate-400">
                                "Record something short, sweet and sovereign. Clips show up here the moment they're posted."
                            </p>
                            <A
                                href="/posts"
                                attr:class="mt-6 inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-5 py-2.5 text-sm shadow-md transition-colors"
                            >
                                "Upload a clip"
                            </A>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <For
                            each=move || video_posts.get()
                            key=|p| p.id.clone()
                            children=move |post: Post| view! { <PostCard post=post /> }
                        />
                    }.into_any()
                }}
            </div>
            <FloatingDock />
        </AppShell>
    }
}
