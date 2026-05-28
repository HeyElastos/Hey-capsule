// SignUp — Rust port of capsules/hey-social/client/src/pages/SignUp.jsx.
//
// In v0.3 the runtime owns the passkey signup flow (it happens in
// System). Hey Social just runs sign-in afterwards. So this page nudges
// the user back to System and offers a link to the sign-in screen.

use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn SignUp() -> impl IntoView {
    view! {
        <section class="min-h-screen flex items-center justify-center px-4 py-8 bg-gradient-to-br from-amber-50 via-rose-50 to-slate-100 dark:from-slate-950 dark:via-slate-950 dark:to-slate-900">
            <div class="max-w-md w-full">
                <div class="rounded-2xl bg-white/85 dark:bg-slate-900/70 backdrop-blur-xl border border-slate-200/70 dark:border-slate-800/70 p-6 shadow-xl">
                    <h1 class="text-2xl font-bold text-slate-900 dark:text-slate-50">
                        "Create your passkey in System"
                    </h1>
                    <p class="mt-3 text-sm text-slate-600 dark:text-slate-400">
                        "Hey uses the same passkey across every app on this node. Open System (the home dock), create a passkey, then come back here to sign in."
                    </p>
                    <A
                        href="/signin"
                        attr:class="mt-6 inline-flex items-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-5 py-2.5 text-sm shadow-md transition-colors"
                    >
                        "Back to sign in"
                    </A>
                </div>
            </div>
        </section>
    }
}
