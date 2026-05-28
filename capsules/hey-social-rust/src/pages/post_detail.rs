// PostDetail — single post view, accessed via /post/:id. Rust port of
// capsules/hey-social/client/src/pages/PostDetail.jsx.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;

use crate::api::posts::{get_post, Post};
use crate::components::{FloatingDock, PostCard};
use crate::pages::misc::AppShell;

#[component]
pub fn PostDetail() -> impl IntoView {
    let params = use_params_map();
    let post: RwSignal<Option<Post>> = RwSignal::new(None);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(String::new());

    Effect::new(move |_| {
        let id = params.read().get("id").map(|s| s.to_string()).unwrap_or_default();
        if id.is_empty() {
            error.set("No post id".into());
            loading.set(false);
            return;
        }
        loading.set(true);
        spawn_local(async move {
            match get_post(&id).await {
                Ok(Some(p)) => {
                    post.set(Some(p));
                }
                Ok(None) => {
                    error.set("Post not found".into());
                }
                Err(e) => {
                    error.set(format!("{e}"));
                }
            }
            loading.set(false);
        });
    });

    view! {
        <AppShell>
            <div class="mx-auto max-w-2xl px-4 pt-6 pb-28 space-y-4">
                {move || {
                    if loading.get() {
                        view! {
                            <div class="rounded-2xl bg-slate-200 dark:bg-slate-800 animate-pulse aspect-square" />
                        }.into_any()
                    } else if !error.get().is_empty() {
                        view! {
                            <div class="rounded-2xl bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900/40 p-4 text-sm text-rose-700 dark:text-rose-300">
                                {error.get()}
                            </div>
                        }.into_any()
                    } else if let Some(p) = post.get() {
                        view! { <PostCard post=p /> }.into_any()
                    } else {
                        view! { <></> }.into_any()
                    }
                }}
            </div>
            <FloatingDock />
        </AppShell>
    }
}
