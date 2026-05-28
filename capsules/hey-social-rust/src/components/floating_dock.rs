// FloatingDock — 1:1 port of capsules/hey-social/client/src/components/
// FloatingDock.jsx. Left-side vertical column, frosted glass, icon-btn
// links to feed / new-post / chat / profile / bell / search.
//
// The .floating-dock CSS class (defined in styles.css) handles all the
// positioning + frosted background. We only emit the structural markup
// + the icon children with the matching class strings.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::icons::{ChatIcon, HomeIcon, PlusIcon, UserIcon};

#[component]
pub fn FloatingDock() -> impl IntoView {
    view! {
        <aside class="floating-dock rounded-[2rem] shadow-2xl shadow-slate-950/40 hidden md:flex md:flex-col">
            <nav class="flex flex-col items-stretch gap-1 p-2">
                <A
                    href="/"
                    attr:class="icon-btn h-12 w-12 mx-auto"
                    attr:title="Feed"
                    attr:aria-label="Feed"
                >
                    <HomeIcon class="h-6 w-6" />
                </A>
                <A
                    href="/posts"
                    attr:class="icon-btn h-12 w-12 mx-auto"
                    attr:title="New post"
                    attr:aria-label="New post"
                >
                    <PlusIcon class="h-6 w-6" />
                </A>
                <A
                    href="/chat"
                    attr:class="icon-btn h-12 w-12 mx-auto"
                    attr:title="Chat"
                    attr:aria-label="Chat"
                >
                    <ChatIcon class="h-6 w-6" />
                </A>
                <A
                    href="/profile"
                    attr:class="icon-btn h-12 w-12 mx-auto"
                    attr:title="Profile"
                    attr:aria-label="Profile"
                >
                    <UserIcon class="h-6 w-6" />
                </A>
            </nav>
        </aside>
    }
}
