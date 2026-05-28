// Home — feed of photo posts. Rust port of capsules/hey-social/client/
// src/pages/Home.jsx (155 lines of React).
//
// Reads the local Hey feed via api::posts::get_posts. Federated receive
// (post.create.v2 → ipfs.get_bytes → IPLD decode → materialize) is the
// next port phase; today's feed only shows posts the local user created.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::posts::{get_posts, Post};
use crate::components::icons::{ArrowRightIcon, CameraIcon};
use crate::components::{FloatingDock, PostCard};
use crate::pages::misc::AppShell;
use crate::session;

#[component]
pub fn Home() -> impl IntoView {
    let posts: RwSignal<Vec<Post>> = RwSignal::new(Vec::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(String::new());

    Effect::new(move |_| {
        loading.set(true);
        error.set(String::new());
        spawn_local(async move {
            match get_posts(50).await {
                Ok(p) => {
                    posts.set(p);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(format!("Unable to load feed: {e}"));
                    loading.set(false);
                }
            }
        });
    });

    let photo_posts = Memo::new(move |_| {
        posts
            .read()
            .iter()
            .filter(|p| {
                p.images
                    .first()
                    .map(|m| m.media_type != "video")
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    view! {
        <AppShell>
            <div class="mx-auto max-w-2xl px-4 pt-6 pb-28 space-y-6">
                {move || if loading.get() {
                    view! { <FeedSkeleton /> }.into_any()
                } else if !error.get().is_empty() {
                    view! {
                        <div class="rounded-2xl bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900/40 p-4 text-sm text-rose-700 dark:text-rose-300">
                            {error.get()}
                        </div>
                    }.into_any()
                } else if photo_posts.read().is_empty() {
                    view! { <EmptyState /> }.into_any()
                } else {
                    view! {
                        <For
                            each=move || photo_posts.get()
                            key=|p| p.id.clone()
                            children=move |post: Post| view! {
                                <PostCard post=post />
                            }
                        />
                    }.into_any()
                }}
            </div>
            <FloatingDock />
        </AppShell>
    }
}

#[component]
fn FeedSkeleton() -> impl IntoView {
    view! {
        <div class="space-y-6">
            {(0..2).map(|_| view! {
                <div class="rounded-2xl overflow-hidden bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
                    <div class="flex items-center gap-3 p-4">
                        <div class="h-10 w-10 rounded-full bg-slate-200 dark:bg-slate-800 animate-pulse" />
                        <div class="space-y-2">
                            <div class="h-3 w-32 rounded bg-slate-200 dark:bg-slate-800 animate-pulse" />
                            <div class="h-2 w-16 rounded bg-slate-200 dark:bg-slate-800 animate-pulse" />
                        </div>
                    </div>
                    <div class="aspect-square bg-slate-200 dark:bg-slate-800 animate-pulse" />
                </div>
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn EmptyState() -> impl IntoView {
    let signed_in = session::current().is_some();
    view! {
        <div class="rounded-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 p-10 text-center">
            <div class="inline-flex h-16 w-16 items-center justify-center rounded-2xl border border-slate-200 dark:border-slate-800 bg-amber-50 dark:bg-amber-500/15 text-amber-600 dark:text-amber-300">
                <CameraIcon class="h-7 w-7" />
            </div>
            <h2 class="mt-5 text-2xl font-semibold text-slate-900 dark:text-slate-50">
                "Your feed is empty"
            </h2>
            <p class="mx-auto mt-3 max-w-sm text-sm text-slate-600 dark:text-slate-400">
                {if signed_in {
                    "Be the first to drop a photo. A view from your window, your morning coffee — anything counts."
                } else {
                    "Sign in to see what your friends are sharing."
                }}
            </p>
            <div class="mt-6">
                <A
                    href=if signed_in { "/posts" } else { "/signin" }
                    attr:class="inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-5 py-2.5 text-sm shadow-md transition-colors"
                >
                    {if signed_in { "Share your first photo" } else { "Get started" }}
                    <ArrowRightIcon class="h-4 w-4" />
                </A>
            </div>
        </div>
    }
}
