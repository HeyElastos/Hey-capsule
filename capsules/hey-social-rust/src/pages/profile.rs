// Profile — view + edit user profile. Rust port of capsules/hey-social/
// client/src/pages/Profile.jsx (664 lines of React; this is a focused
// subset: identity card, edit name/bio, list of own posts).
//
// Profile editing writes to BOTH the Hey-local profile.json AND the
// shared identity at .AppData/{ElastOS,}/Identity/profile.json — see
// api::profile::update_profile for the dual-write semantics.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::api::posts::{get_user_posts, Post};
use crate::api::profile::{ensure_profile, update_profile, Profile as ProfileRecord, ProfileUpdate};
use crate::components::{FloatingDock, PostCard, TopHeader};
use crate::session;

#[component]
pub fn Profile() -> impl IntoView {
    let params = use_params_map();
    let profile: RwSignal<Option<ProfileRecord>> = RwSignal::new(None);
    let posts: RwSignal<Vec<Post>> = RwSignal::new(Vec::new());
    let editing = RwSignal::new(false);
    let edit_name = RwSignal::new(String::new());
    let edit_bio = RwSignal::new(String::new());
    let saving = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    Effect::new(move |_| {
        let did_param = params
            .read()
            .get("did")
            .map(|s| s.to_string())
            .unwrap_or_default();
        let me_did = session::current().map(|s| s.did_key).unwrap_or_default();
        spawn_local(async move {
            // For "me" we ensure-and-backfill; for anyone else we render
            // best-effort from get_user_posts (the Rust port doesn't have
            // remote-profile-fetch yet — that's an api::profile follow-up).
            if did_param.is_empty() || did_param == me_did {
                if let Ok(me) = ensure_profile().await {
                    edit_name.set(me.name.clone());
                    edit_bio.set(me.bio.clone());
                    profile.set(Some(me));
                }
            }
            let target = if did_param.is_empty() {
                me_did.clone()
            } else {
                did_param.clone()
            };
            if !target.is_empty() {
                let p = get_user_posts(&target).await.unwrap_or_default();
                posts.set(p);
            }
        });
    });

    let on_name_input = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(input) = target.dyn_into::<HtmlInputElement>() {
                edit_name.set(input.value());
            }
        }
    };
    let on_bio_input = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(ta) = target.dyn_into::<web_sys::HtmlTextAreaElement>() {
                edit_bio.set(ta.value());
            }
        }
    };

    let save = move |_| {
        if saving.get() {
            return;
        }
        let name = edit_name.get();
        let bio = edit_bio.get();
        saving.set(true);
        error.set(String::new());
        spawn_local(async move {
            match update_profile(ProfileUpdate {
                name: Some(name),
                bio: Some(bio),
                avatar: None,
            })
            .await
            {
                Ok(p) => {
                    profile.set(Some(p));
                    editing.set(false);
                }
                Err(e) => error.set(format!("{e}")),
            }
            saving.set(false);
        });
    };

    let me_did = session::current().map(|s| s.did_key).unwrap_or_default();
    let is_self_view = Memo::new(move |_| {
        let p = params.read();
        let did_param = p.get("did").unwrap_or_default();
        did_param.is_empty() || did_param == me_did
    });

    view! {
        <>
            <TopHeader />
            <FloatingDock />
            <div class="mx-auto max-w-2xl space-y-6 px-4 py-10 sm:px-6">
                <header class="frosted-card p-6 animate-fade-up">
                    {move || match profile.get() {
                        Some(me) => view! {
                            <div class="flex items-start gap-4">
                                <div class="h-16 w-16 rounded-full bg-gradient-to-br from-amber-400 to-rose-400 grid place-items-center text-white text-xl font-bold">
                                    {me.name.chars().next().map(|c| c.to_uppercase().next().unwrap_or(c).to_string()).unwrap_or_else(|| "?".into())}
                                </div>
                                <div class="min-w-0 flex-1">
                                    {move || if editing.get() && is_self_view.get() {
                                        view! {
                                            <div class="space-y-2">
                                                <input
                                                    class="w-full rounded-lg bg-slate-100 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 px-3 py-2 text-sm"
                                                    type="text"
                                                    maxlength="30"
                                                    prop:value=move || edit_name.get()
                                                    on:input=on_name_input
                                                />
                                                <textarea
                                                    class="w-full rounded-lg bg-slate-100 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 px-3 py-2 text-sm"
                                                    rows="2"
                                                    maxlength="280"
                                                    placeholder="Bio"
                                                    prop:value=move || edit_bio.get()
                                                    on:input=on_bio_input
                                                />
                                                {move || {
                                                    let m = error.get();
                                                    if m.is_empty() { view! { <></> }.into_any() }
                                                    else { view! { <p class="text-xs text-rose-500">{m}</p> }.into_any() }
                                                }}
                                                <div class="flex gap-2">
                                                    <button
                                                        type="button"
                                                        on:click=save
                                                        prop:disabled=move || saving.get()
                                                        class="rounded-full bg-amber-500 hover:bg-amber-600 disabled:bg-slate-300 dark:disabled:bg-slate-700 text-white font-semibold px-4 py-2 text-xs"
                                                    >
                                                        {move || if saving.get() { "Saving…" } else { "Save" }}
                                                    </button>
                                                    <button
                                                        type="button"
                                                        on:click=move |_| { editing.set(false); }
                                                        class="rounded-full bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-semibold px-4 py-2 text-xs"
                                                    >
                                                        "Cancel"
                                                    </button>
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        let me_view = me.clone();
                                        view! {
                                            <>
                                                <h1 class="text-xl font-bold text-slate-900 dark:text-slate-50 truncate">
                                                    {me_view.name.clone()}
                                                </h1>
                                                <p class="mt-1 text-[11px] font-mono text-slate-500 dark:text-slate-400 break-all">
                                                    {me_view.did_key.clone()}
                                                </p>
                                                {if me_view.bio.is_empty() {
                                                    view! { <></> }.into_any()
                                                } else {
                                                    view! {
                                                        <p class="mt-2 text-sm text-slate-700 dark:text-slate-300 whitespace-pre-wrap">
                                                            {me_view.bio.clone()}
                                                        </p>
                                                    }.into_any()
                                                }}
                                                {move || if is_self_view.get() {
                                                    view! {
                                                        <button
                                                            type="button"
                                                            on:click=move |_| { editing.set(true); }
                                                            class="mt-3 inline-flex items-center gap-1 rounded-full bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 px-3 py-1.5 text-xs font-medium"
                                                        >
                                                            "Edit profile"
                                                        </button>
                                                    }.into_any()
                                                } else {
                                                    view! { <></> }.into_any()
                                                }}
                                            </>
                                        }.into_any()
                                    }}
                                </div>
                            </div>
                        }.into_any(),
                        None => view! {
                            <p class="text-sm text-slate-500 dark:text-slate-400">"Loading profile…"</p>
                        }.into_any(),
                    }}
                </header>

                <section>
                    <h2 class="px-1 mb-3 text-xs uppercase tracking-wider text-muted">
                        "Posts"
                    </h2>
                    {move || {
                        let list = posts.get();
                        if list.is_empty() {
                            view! {
                                <p class="frosted-card p-6 text-sm text-muted text-center">
                                    "No posts yet."
                                </p>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-4">
                                    <For
                                        each=move || posts.get()
                                        key=|p| p.id.clone()
                                        children=move |p: Post| view! {
                                            <div class="animate-fade-up">
                                                <PostCard post=p />
                                            </div>
                                        }
                                    />
                                </div>
                            }.into_any()
                        }
                    }}
                </section>
            </div>
        </>
    }
}
