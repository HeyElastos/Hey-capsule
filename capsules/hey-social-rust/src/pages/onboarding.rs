// Onboarding — first-run welcome card. The React reference's onboarding
// is a multi-step wizard (capsules/hey-social/client/src/pages/Onboarding.jsx
// is 367 lines); the Rust port stays minimal — a single welcome screen
// that sends the user to /home. Polish parity is a follow-up.

use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn Onboarding() -> impl IntoView {
    view! {
        <section class="min-h-screen flex items-center justify-center px-4 py-8 bg-gradient-to-br from-amber-50 via-rose-50 to-slate-100 dark:from-slate-950 dark:via-slate-950 dark:to-slate-900">
            <div class="max-w-md w-full">
                <div class="rounded-2xl bg-white/85 dark:bg-slate-900/70 backdrop-blur-xl border border-slate-200/70 dark:border-slate-800/70 p-6 shadow-xl">
                    <h1 class="text-2xl font-bold text-slate-900 dark:text-slate-50">
                        "Welcome to Hey"
                    </h1>
                    <p class="mt-3 text-sm text-slate-600 dark:text-slate-400">
                        "You're signed in. Your DID is anchored to your passkey — every Hey app on this node will recognize you automatically."
                    </p>
                    <A
                        href="/home"
                        attr:class="mt-6 inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-5 py-2.5 text-sm shadow-md transition-colors"
                    >
                        "Go to feed"
                    </A>
                </div>
            </div>
        </section>
    }
}
