// Landing — public splash. Sends signed-out visitors to /signin, signed-in
// users to /home automatically. Mirrors capsules/hey-social/client/src/
// pages/Landing.jsx (the SVG FloatingScene is omitted; the splash is a
// simple gradient + CTA in the Rust port).

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

use crate::components::icons::ArrowRightIcon;
use crate::session;

#[component]
pub fn Landing() -> impl IntoView {
    let navigate = use_navigate();
    Effect::new({
        let navigate = navigate.clone();
        move |_| {
            if session::current().is_some() {
                navigate("/home", NavigateOptions::default());
            }
        }
    });
    let go_signin = {
        let navigate = navigate.clone();
        move |_| navigate("/signin", NavigateOptions::default())
    };
    view! {
        <section class="min-h-screen flex items-center justify-center px-4 py-8 bg-gradient-to-br from-amber-50 via-rose-50 to-slate-100 dark:from-slate-950 dark:via-slate-950 dark:to-slate-900">
            <div class="max-w-md w-full text-center">
                <h1 class="text-5xl font-bold tracking-tight text-slate-900 dark:text-slate-50">
                    "Hey " <span class="text-amber-500">"Social"</span>
                </h1>
                <p class="mt-3 text-sm text-slate-600 dark:text-slate-400 max-w-sm mx-auto">
                    "Federated photos, clips and chat on Elastos. End-to-end encrypted. No backend in the middle. Your identity is one passkey across every Hey app."
                </p>
                <button
                    type="button"
                    on:click=go_signin
                    class="mt-8 inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-6 py-3.5 text-base shadow-lg transition-colors"
                >
                    "Get started"
                    <ArrowRightIcon class="h-4 w-4" />
                </button>
            </div>
        </section>
    }
}
