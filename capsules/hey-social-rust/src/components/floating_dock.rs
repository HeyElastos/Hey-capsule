// FloatingDock — responsive nav.
//   * Desktop (md+): left-side vertical frosted column, fixed at top:50%.
//   * Mobile (< md): bottom horizontal frosted bar, full-width with
//     safe-area padding so it sits above the iOS gesture line.
//
// The styles.css .floating-dock class hardcodes desktop position, so we
// don't use it here — we emit Tailwind utilities directly and pick up
// the frosted backdrop via bg-white/* + backdrop-blur-xl. Active-route
// icon gets the .is-active glow (defined in styles.css).

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::components::icons::{ChatIcon, HomeIcon, PlusIcon, UserIcon};
use crate::components::NavLink;

#[component]
pub fn FloatingDock() -> impl IntoView {
    let location = use_location();
    let active = move |p: &str| -> bool {
        let path = location.pathname.get();
        match p {
            "/" => path == "/" || path == "/home",
            "/posts" => path == "/posts",
            "/chat" => path == "/chat",
            "/profile" => path.starts_with("/profile"),
            _ => path == p,
        }
    };

    let icon_class = move |is_active: bool| -> String {
        if is_active {
            "icon-btn is-active h-12 w-12 inline-flex items-center justify-center".into()
        } else {
            "icon-btn h-12 w-12 inline-flex items-center justify-center".into()
        }
    };

    view! {
        <aside class="
            fixed z-40
            inset-x-3 bottom-3
            md:inset-auto md:bottom-auto md:left-4 md:top-1/2 md:-translate-y-1/2
            pb-[env(safe-area-inset-bottom)]
        ">
            <div class="
                mx-auto md:mx-0
                max-w-md md:w-20
                rounded-3xl md:rounded-[2rem]
                border border-white/12 dark:border-white/12
                bg-white/80 dark:bg-white/8
                backdrop-blur-xl
                shadow-2xl shadow-slate-950/40
            ">
                <nav class="
                    flex items-stretch gap-1 p-2
                    flex-row justify-around
                    md:flex-col md:justify-start
                ">
                    <NavLink
                        href="/"
                        class=icon_class(active("/"))
                        title="Feed"
                        aria_label="Feed"
                    >
                        <HomeIcon class="h-6 w-6" />
                    </NavLink>
                    <NavLink
                        href="/posts"
                        class=icon_class(active("/posts"))
                        title="New post"
                        aria_label="New post"
                    >
                        <PlusIcon class="h-6 w-6" />
                    </NavLink>
                    <NavLink
                        href="/chat"
                        class=icon_class(active("/chat"))
                        title="Chat"
                        aria_label="Chat"
                    >
                        <ChatIcon class="h-6 w-6" />
                    </NavLink>
                    <NavLink
                        href="/profile"
                        class=icon_class(active("/profile"))
                        title="Profile"
                        aria_label="Profile"
                    >
                        <UserIcon class="h-6 w-6" />
                    </NavLink>
                </nav>
            </div>
        </aside>
    }
}
