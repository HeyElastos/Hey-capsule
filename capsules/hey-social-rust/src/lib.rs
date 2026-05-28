use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

pub mod api;
pub mod components;
pub mod events;
pub mod identity;
pub mod pages;
pub mod passkey;
pub mod runtime;
pub mod session;
pub mod shell;

#[component]
pub fn App() -> impl IntoView {
    // Pre-warm capability tokens for the providers we'll touch. The runtime
    // auto-grants any resource declared in capsule.json, so this is one
    // round-trip per provider on first launch and zero on every subsequent
    // navigation (cached in sessionStorage).
    spawn_local(async {
        runtime::acquire_boot_capabilities().await;
    });

    view! {
        <Router>
            <main class="min-h-screen text-primary">
                <Routes fallback=|| view! { <pages::NotFound /> }>
                    // React route paths (canonical):
                    <Route path=path!("/") view=pages::Home />
                    <Route path=path!("/videos") view=pages::Clips />
                    <Route path=path!("/posts") view=pages::Posts />
                    <Route path=path!("/p/:id") view=pages::PostDetail />
                    <Route path=path!("/v/:id") view=pages::VideoPlayer />
                    <Route path=path!("/profile") view=pages::Profile />
                    <Route path=path!("/profile/:did") view=pages::Profile />
                    <Route path=path!("/chat") view=pages::Chat />
                    <Route path=path!("/welcome") view=pages::Onboarding />
                    <Route path=path!("/signup") view=pages::SignUp />
                    <Route path=path!("/signin") view=pages::SignIn />
                    // Backwards-compat aliases for any links pointing at old paths:
                    <Route path=path!("/home") view=pages::Home />
                    <Route path=path!("/clips") view=pages::Clips />
                    <Route path=path!("/post/:id") view=pages::PostDetail />
                    <Route path=path!("/video/:id") view=pages::VideoPlayer />
                    <Route path=path!("/onboarding") view=pages::Onboarding />
                </Routes>
            </main>
        </Router>
    }
}
