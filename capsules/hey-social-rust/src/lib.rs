use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

mod components;
mod identity;
mod pages;
mod passkey;
mod runtime;
mod session;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main class="min-h-screen bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100">
                <components::SignInGate />
                <Routes fallback=|| view! { <pages::NotFound /> }>
                    <Route path=path!("/") view=pages::Landing />
                    <Route path=path!("/signin") view=pages::SignIn />
                    <Route path=path!("/signup") view=pages::SignUp />
                    <Route path=path!("/onboarding") view=pages::Onboarding />
                    <Route path=path!("/home") view=pages::Home />
                    <Route path=path!("/clips") view=pages::Clips />
                    <Route path=path!("/post/:id") view=pages::PostDetail />
                    <Route path=path!("/video/:id") view=pages::VideoPlayer />
                    <Route path=path!("/profile/:did") view=pages::Profile />
                    <Route path=path!("/chat") view=pages::Chat />
                </Routes>
            </main>
        </Router>
    }
}
