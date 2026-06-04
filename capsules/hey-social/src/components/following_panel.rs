// FollowingPanel — the social graph popup, openable from the FloatingDock.
// Two tabs: "Following" (people you follow) and "Followers" (people who follow
// you), each with a live count. Rows link to /profile/<did>. Distinct from the
// Contacts panel, which lists DM/chat contacts. Uses the shared <Modal> shell.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::profile::{list_followers, list_following, FollowView};
use crate::app_modals::AppModals;
use crate::components::icons::UsersIcon;
use crate::components::{Modal, NavLink};

#[component]
pub fn FollowingPanel(open: RwSignal<bool>) -> impl IntoView {
    let following: RwSignal<Vec<FollowView>> = RwSignal::new(Vec::new());
    let followers: RwSignal<Vec<FollowView>> = RwSignal::new(Vec::new());
    let loaded = RwSignal::new(false);
    // false = Following tab, true = Followers tab.
    let show_followers = RwSignal::new(false);
    let modals = use_context::<AppModals>().unwrap_or_default();

    // (Re)load both lists whenever the panel opens so counts + rows are fresh.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        loaded.set(false);
        spawn_local(async move {
            following.set(list_following().await);
            followers.set(list_followers().await);
            loaded.set(true);
        });
    });

    let tab_class = move |is_active: bool| -> String {
        if is_active {
            "flex-1 rounded-xl px-3 py-1.5 text-sm font-semibold bg-accent text-accent-text transition-colors".into()
        } else {
            "flex-1 rounded-xl px-3 py-1.5 text-sm font-medium text-muted hover:bg-white/10 transition-colors".into()
        }
    };

    view! {
        <Modal open=open>
            <div class="frosted-card frosted-card-strong p-5 max-h-[70vh] overflow-y-auto">
                <header class="flex items-baseline justify-between mb-3">
                    <h3 class="logo-handwritten text-4xl text-primary">"Network"</h3>
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
                <div class="flex gap-1.5 mb-3 rounded-2xl bg-white/5 p-1">
                    <button type="button" class=move || tab_class(!show_followers.get()) on:click=move |_| show_followers.set(false)>
                        "Following " {move || following.get().len()}
                    </button>
                    <button type="button" class=move || tab_class(show_followers.get()) on:click=move |_| show_followers.set(true)>
                        "Followers " {move || followers.get().len()}
                    </button>
                </div>
                {move || {
                    let list = if show_followers.get() { followers.get() } else { following.get() };
                    if list.is_empty() {
                        if !loaded.get() {
                            view! { <></> }.into_any()
                        } else {
                            let (msg, cta, is_following_tab) = if show_followers.get() {
                                ("No followers yet.", "", false)
                            } else {
                                ("You're not following anyone yet.", "Follow someone", true)
                            };
                            view! {
                                <div class="text-center py-10">
                                    <div class="float-soft inline-flex h-14 w-14 items-center justify-center rounded-2xl border border-white/20 bg-white/10 backdrop-blur-xl text-accent">
                                        <UsersIcon class="h-6 w-6" />
                                    </div>
                                    <p class="mt-4 text-sm text-muted">{msg}</p>
                                    {if is_following_tab {
                                        view! {
                                            <button
                                                type="button"
                                                on:click=move |_| { open.set(false); modals.add_friend_open.set(true); }
                                                class="unfrost mt-4 inline-flex items-center gap-2 rounded-full bg-accent px-5 py-2 text-xs font-semibold text-accent-text hover:bg-amber-300 transition-colors"
                                            >
                                                {cta}
                                            </button>
                                        }.into_any()
                                    } else { view! { <></> }.into_any() }}
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
