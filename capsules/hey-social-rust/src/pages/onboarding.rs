// Onboarding — first-run welcome card. The React reference's onboarding
// is a multi-step wizard (capsules/hey-social/client/src/pages/Onboarding.jsx
// is 367 lines); the Rust port stays minimal — a single welcome screen
// that sends the user to /home. Polish parity is a follow-up.

use leptos::prelude::*;

use crate::components::NavLink;

#[component]
pub fn Onboarding() -> impl IntoView {
    view! {
        <section class="relative min-h-[80vh] flex items-center justify-center pl-24 pr-3 py-6 sm:pl-28 sm:pr-6 sm:py-10">
            <div class="w-full max-w-2xl">
                <div class="frosted-card p-10 sm:p-14 text-center animate-fade-up">
                    <h1 class="logo-handwritten text-5xl sm:text-6xl text-primary">
                        "Welcome to Hey"
                    </h1>
                    <p class="mt-5 text-base text-muted max-w-lg mx-auto leading-7">
                        "You're signed in. Your DID is anchored to your passkey — every Hey app on this node will recognize you automatically."
                    </p>
                    <NavLink
                        href="/"
                        class="unfrost mt-8 inline-flex items-center gap-2 rounded-full bg-accent px-6 py-3 text-base font-semibold text-accent-text shadow-md transition hover:bg-amber-300"
                    >
                        "Go to feed"
                    </NavLink>
                </div>
            </div>
        </section>
    }
}
