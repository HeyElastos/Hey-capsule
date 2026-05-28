// Sticky top header — Hey wordmark on left, photo/video tabs in center
// with .is-active glow, bell + search + add-friend + logout cluster on
// right. Each modal trigger toggles a shared RwSignal that the App-level
// modals (AddFriendModal / SearchModal / NotificationPanel) read.

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_location, use_navigate};
use leptos_router::NavigateOptions;

use crate::api::notifications;
use crate::app_modals::AppModals;
use crate::components::icons::{BellIcon, CameraIcon, LogoutIcon, PlusIcon, SearchIcon, VideoIcon};
use crate::components::NavLink;
use crate::session;

fn current_base() -> String {
    let Some(win) = web_sys::window() else { return String::new(); };
    let Ok(path) = win.location().pathname() else { return String::new(); };
    let Some(idx) = path.find("/apps/") else { return String::new(); };
    let after = &path[idx + 6..];
    let end = after.find('/').map(|j| idx + 6 + j).unwrap_or(path.len());
    path[..end].to_string()
}

#[component]
pub fn TopHeader() -> impl IntoView {
    let modals = use_context::<AppModals>().unwrap_or_default();
    let notifications_open = modals.notifications_open;
    let search_open = modals.search_open;
    let add_friend_open = modals.add_friend_open;
    let location = use_location();
    let navigate = use_navigate();
    let base = current_base();

    let is_videos = move || {
        let p = location.pathname.get();
        p.starts_with("/videos") || p == "/clips"
    };

    let logout = {
        let navigate = navigate.clone();
        move |_| {
            session::clear();
            navigate("/", NavigateOptions::default());
        }
    };

    let click_to = {
        let navigate = navigate.clone();
        move |path: &'static str| {
            let navigate = navigate.clone();
            move |ev: MouseEvent| {
                if ev.default_prevented()
                    || ev.button() != 0
                    || ev.meta_key()
                    || ev.ctrl_key()
                    || ev.shift_key()
                    || ev.alt_key()
                { return; }
                ev.prevent_default();
                navigate(path, NavigateOptions::default());
            }
        }
    };

    // Live unread-count for the bell badge. Re-poll every 10s.
    let unread = RwSignal::new(0usize);
    Effect::new(move |_| {
        spawn_local(async move {
            loop {
                if session::current().is_some() {
                    let n = notifications::unread_count().await;
                    unread.set(n);
                }
                wait_10s().await;
            }
        });
    });

    view! {
        <header class="sticky top-0 z-30 bg-surface-soft/95 backdrop-blur-xl shadow-[0_16px_40px_-18px_rgba(0,0,0,0.15)]">
            <div class="mx-auto flex max-w-6xl items-center justify-between px-4 py-3 sm:px-6">
                <NavLink
                    href="/"
                    class="text-3xl font-semibold text-primary logo-handwritten sm:text-5xl"
                >
                    "Hey"
                </NavLink>

                <nav class="flex flex-1 items-center justify-center gap-8 text-sm sm:gap-12">
                    <a
                        href=format!("{}/", base)
                        class="icon-btn tab-icon"
                        class:is-active=move || !is_videos()
                        aria-label="Photos"
                        on:click=click_to.clone()("/")
                    >
                        <CameraIcon class="h-6 w-6" />
                    </a>
                    <a
                        href=format!("{}/videos", base)
                        class="icon-btn tab-icon"
                        class:is-active=is_videos
                        aria-label="Videos"
                        on:click=click_to.clone()("/videos")
                    >
                        <VideoIcon class="h-6 w-6" />
                    </a>
                </nav>

                <div class="flex items-center gap-1">
                    <button
                        type="button"
                        on:click=move |_| search_open.set(true)
                        class="icon-btn"
                        aria-label="Find user"
                        title="Find user"
                    >
                        <SearchIcon class="h-5 w-5" />
                    </button>
                    <button
                        type="button"
                        on:click=move |_| add_friend_open.set(true)
                        class="icon-btn"
                        aria-label="Add friend"
                        title="Add friend"
                    >
                        <PlusIcon class="h-5 w-5" />
                    </button>
                    <button
                        type="button"
                        on:click=move |_| notifications_open.set(true)
                        class="icon-btn relative"
                        aria-label="Notifications"
                        title="Notifications"
                    >
                        <BellIcon class="h-5 w-5" />
                        {move || {
                            let n = unread.get();
                            if n == 0 { view! { <></> }.into_any() } else {
                                let label = if n > 9 { "9+".to_string() } else { n.to_string() };
                                view! {
                                    <span class="pointer-events-none absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-rose-500 px-1 text-[10px] font-bold leading-none text-white">
                                        {label}
                                    </span>
                                }.into_any()
                            }
                        }}
                    </button>
                    <button
                        type="button"
                        on:click=logout
                        class="icon-btn"
                        aria-label="Log out"
                        title="Log out"
                    >
                        <LogoutIcon class="h-5 w-5" />
                    </button>
                </div>
            </div>
        </header>
    }
}

async fn wait_10s() {
    let win = web_sys::window().unwrap();
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let _ = win
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10_000);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
