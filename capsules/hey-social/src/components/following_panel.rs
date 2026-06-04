// FollowingPanel — centered popup listing the people you FOLLOW (the social
// graph, from follows.json), openable from the FloatingDock. Distinct from the
// Contacts panel, which lists DM/chat contacts. Each row links to that user's
// profile (/profile/<did>) so you can see their posts, then closes the panel.
//
// Mirrors ContactsPanel's structure (shared <Modal> shell, same row styling).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::profile::{list_following, FollowView};
use crate::app_modals::AppModals;
use crate::components::icons::UsersIcon;
use crate::components::{Modal, NavLink};

#[component]
pub fn FollowingPanel(open: RwSignal<bool>) -> impl IntoView {
    let following: RwSignal<Vec<FollowView>> = RwSignal::new(Vec::new());
    let loaded = RwSignal::new(false);
    let modals = use_context::<AppModals>().unwrap_or_default();

    // (Re)load every time the panel opens so it reflects newly-followed users.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        loaded.set(false);
        spawn_local(async move {
            following.set(list_following().await);
            loaded.set(true);
        });
    });

    view! {
        <Modal open=open>
            <div class="frosted-card frosted-card-strong p-5 max-h-[70vh] overflow-y-auto">
                <header class="flex items-baseline justify-between mb-3">
                    <h3 class="logo-handwritten text-4xl text-primary">"Following"</h3>
                    <button
                        type="button"
                        on:click=move |_| open.set(false)
                        class="icon-btn-ghost"
                        aria-label="Close"
                    >
                        <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M18 6 6 18M6 6l12 12" />
                        </svg>
                    </button>
                </header>
                {move || {
                    let list = following.get();
                    if list.is_empty() {
                        if !loaded.get() {
                            view! { <></> }.into_any()
                        } else {
                            view! {
                                <div class="text-center py-10">
                                    <div class="float-soft inline-flex h-14 w-14 items-center justify-center rounded-2xl border border-white/20 bg-white/10 backdrop-blur-xl text-accent">
                                        <UsersIcon class="h-6 w-6" />
                                    </div>
                                    <p class="mt-4 text-sm text-muted">"You're not following anyone yet."</p>
                                    <button
                                        type="button"
                                        on:click=move |_| {
                                            open.set(false);
                                            modals.add_friend_open.set(true);
                                        }
                                        class="unfrost mt-4 inline-flex items-center gap-2 rounded-full bg-accent px-5 py-2 text-xs font-semibold text-accent-text hover:bg-amber-300 transition-colors"
                                    >
                                        "Follow someone"
                                    </button>
                                </div>
                            }.into_any()
                        }
                    } else {
                        view! {
                            <ul class="space-y-1.5">
                                {list.into_iter().map(|f| view! { <FollowRow f=f on_navigate=move || open.set(false) /> }).collect::<Vec<_>>()}
                            </ul>
                        }.into_any()
                    }
                }}
            </div>
        </Modal>
    }
}

#[component]
fn FollowRow(
    f: FollowView,
    on_navigate: impl Fn() + 'static + Send + Sync + Clone,
) -> impl IntoView {
    let name = if f.name.trim().is_empty() { short_did(&f.did) } else { f.name.clone() };
    let avatar = avatar_letters(&name);
    let sub = short_did(&f.did);
    let href = format!("/profile/{}", f.did);

    view! {
        <li on:click=move |_| on_navigate()>
            <NavLink
                href=href
                class="flex items-center gap-3 rounded-2xl px-3 py-2.5 hover:bg-white/10 transition-colors"
            >
                <span class="flex h-10 w-10 flex-none items-center justify-center rounded-full bg-gradient-to-br from-accent to-amber-600 text-accent-text text-sm font-bold shadow-sm">
                    {avatar}
                </span>
                <div class="flex-1 min-w-0">
                    <span class="block text-sm font-medium text-primary truncate">{name}</span>
                    <span class="block text-xs text-muted truncate">{sub}</span>
                </div>
            </NavLink>
        </li>
    }
}

fn avatar_letters(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect::<String>()
        .to_uppercase()
}

fn short_did(did: &str) -> String {
    let s = did.strip_prefix("did:key:z").unwrap_or(did);
    if s.len() > 14 {
        format!("{}…", s.chars().take(14).collect::<String>())
    } else {
        s.into()
    }
}
