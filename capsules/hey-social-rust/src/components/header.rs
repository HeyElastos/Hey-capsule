// Sticky top header — port of capsules/hey-social/client/src/App.jsx's
// header block. Hey wordmark on left, photo/video tabs in the center
// (with .is-active glow), logout button on right.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_location, use_navigate};
use leptos_router::NavigateOptions;

use crate::components::icons::{CameraIcon, LogoutIcon, VideoIcon};
use crate::session;

#[component]
pub fn TopHeader() -> impl IntoView {
    let location = use_location();
    let navigate = use_navigate();

    let is_videos = move || location.pathname.get().starts_with("/videos")
        || location.pathname.get() == "/clips";

    let logout = move |_| {
        session::clear();
        navigate("/", NavigateOptions::default());
    };

    view! {
        <header class="sticky top-0 z-30 bg-surface-soft/95 backdrop-blur-xl shadow-[0_16px_40px_-18px_rgba(0,0,0,0.15)]">
            <div class="mx-auto flex max-w-6xl items-center justify-between px-4 py-3 sm:px-6">
                <A
                    href="/"
                    attr:class="text-3xl font-semibold text-primary logo-handwritten sm:text-5xl"
                >
                    "Hey"
                </A>

                <nav class="flex flex-1 items-center justify-center gap-8 text-sm sm:gap-12">
                    <A
                        href="/"
                        attr:class=move || format!("icon-btn tab-icon {}", if is_videos() { "" } else { "is-active" })
                        attr:aria-label="Photos"
                    >
                        <CameraIcon class="h-6 w-6" />
                    </A>
                    <A
                        href="/videos"
                        attr:class=move || format!("icon-btn tab-icon {}", if is_videos() { "is-active" } else { "" })
                        attr:aria-label="Videos"
                    >
                        <VideoIcon class="h-6 w-6" />
                    </A>
                </nav>

                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        on:click=logout
                        class="icon-btn"
                        aria-label="Log out"
                    >
                        <LogoutIcon class="h-5 w-5" />
                    </button>
                </div>
            </div>
        </header>
    }
}
