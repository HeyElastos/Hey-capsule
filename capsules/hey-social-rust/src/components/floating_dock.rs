// Bottom-floating navigation dock. Rust port of capsules/hey-social/client/
// src/components/FloatingDock.jsx (trimmed; same routes, simpler chrome).
//
// Shows on every signed-in page. The five primary destinations are Home,
// Clips, Posts (create), Chat, and Profile (current user). A SignInGate
// elsewhere makes sure unauthed users never see this — but we still bail
// out gracefully if session::current() is None.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use crate::components::icons::{ChatIcon, ClipsIcon, HomeIcon, PlusIcon, UserIcon};
use crate::session;

#[component]
pub fn FloatingDock() -> impl IntoView {
    let location = use_location();
    let me = session::current();
    let did = me.as_ref().map(|s| s.did_key.clone()).unwrap_or_default();
    let profile_href = if did.is_empty() {
        "/signin".to_string()
    } else {
        format!("/profile/{did}")
    };

    let is_active = move |path: &str| {
        let p = location.pathname.get();
        if path == "/home" {
            p == "/home" || p == "/"
        } else {
            p.starts_with(path)
        }
    };

    let item_class = move |active: bool| -> String {
        if active {
            "flex flex-col items-center gap-0.5 text-amber-500".into()
        } else {
            "flex flex-col items-center gap-0.5 text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 transition-colors".into()
        }
    };

    view! {
        <nav class="fixed inset-x-0 bottom-3 z-30 px-4 pointer-events-none">
            <div class="mx-auto max-w-md rounded-full bg-white/85 dark:bg-slate-900/85 backdrop-blur-xl border border-slate-200/70 dark:border-slate-800/70 shadow-lg pointer-events-auto">
                <ul class="flex items-center justify-between px-4 py-2 text-[10px] font-medium">
                    <li>
                        <A href="/home" attr:class=item_class(is_active("/home"))>
                            <HomeIcon class="h-5 w-5" />
                            <span>"Feed"</span>
                        </A>
                    </li>
                    <li>
                        <A href="/clips" attr:class=item_class(is_active("/clips"))>
                            <ClipsIcon class="h-5 w-5" />
                            <span>"Clips"</span>
                        </A>
                    </li>
                    <li>
                        <A href="/posts" attr:class=item_class(is_active("/posts"))>
                            <span class="flex h-9 w-9 items-center justify-center rounded-full bg-amber-500 text-white shadow-md">
                                <PlusIcon class="h-5 w-5" />
                            </span>
                        </A>
                    </li>
                    <li>
                        <A href="/chat" attr:class=item_class(is_active("/chat"))>
                            <ChatIcon class="h-5 w-5" />
                            <span>"Chat"</span>
                        </A>
                    </li>
                    <li>
                        <A href=profile_href attr:class=item_class(is_active("/profile"))>
                            <UserIcon class="h-5 w-5" />
                            <span>"Me"</span>
                        </A>
                    </li>
                </ul>
            </div>
        </nav>
    }
}
