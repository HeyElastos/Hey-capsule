use leptos::prelude::*;

use crate::components::NavLink;

#[component]
pub fn Onboarding() -> impl IntoView {
    view! {
        <section class="relative min-h-[80vh] flex items-center justify-center pl-24 pr-3 py-6 sm:pl-28 sm:pr-6 sm:py-10 overflow-hidden">
            <OnboardingScene />
            <div class="relative z-10 w-full max-w-2xl">
                <div class="frosted-card p-10 sm:p-14 text-center animate-fade-up">
                    <h1 class="logo-handwritten text-5xl sm:text-6xl text-primary">
                        "Welcome to Hey"
                    </h1>
                    <p class="mt-5 text-base text-muted max-w-lg mx-auto leading-7">
                        "You're signed in. Your DID is anchored to your passkey — every Hey app on this node will recognize you automatically. Photos pin to IPFS, posts federate via Carrier, DMs are wrapped in ML-KEM-768 + X25519 hybrid post-quantum crypto."
                    </p>
                    <NavLink
                        href="/"
                        class="unfrost mt-8 inline-flex items-center gap-2 rounded-full bg-accent px-7 py-3 text-base font-semibold text-accent-text shadow-md transition hover:bg-amber-300"
                    >
                        "Go to feed"
                    </NavLink>
                </div>
            </div>
        </section>
    }
}

// Background scene: "warp into screen" effect. Symbols spawn tiny at
// the viewport center, then grow + rotate while drifting outward toward
// one of 8 exit directions, like the user is being sucked through the
// scene. The .warp-* keyframes in welcome-animations.css include
// translate(-50%, -50%) so each symbol re-centers on every frame; only
// the final translate carries it past the edge.
//
// Staggered negative animation-delays mean something is always emerging
// from the center while others fade past the screen edges.
#[component]
fn OnboardingScene() -> impl IntoView {
    view! {
        <div
            class="pointer-events-none absolute inset-0 overflow-hidden"
            aria-hidden="true"
            style="perspective: 1200px;"
        >
            // Slow-drifting gradient blobs anchor the scene so the
            // warping symbols don't feel adrift on flat black.
            <div
                class="absolute glow-drift"
                style="top: 8%; left: 6%; width: 380px; height: 380px;
                       background: radial-gradient(circle closest-side at center,
                         rgba(212,184,75,0.65) 0%, rgba(212,184,75,0.22) 40%, transparent 75%);
                       filter: blur(75px);"
            />
            <div
                class="absolute glow-drift"
                style="bottom: 6%; right: 4%; width: 480px; height: 480px;
                       background: radial-gradient(circle closest-side at center,
                         rgba(96,165,250,0.55) 0%, rgba(96,165,250,0.18) 40%, transparent 75%);
                       filter: blur(90px); animation-delay: -3s;"
            />
            <div
                class="absolute glow-drift"
                style="top: 42%; right: 22%; width: 300px; height: 300px;
                       background: radial-gradient(circle closest-side at center,
                         rgba(244,114,182,0.50) 0%, rgba(244,114,182,0.18) 40%, transparent 75%);
                       filter: blur(70px); animation-delay: -6s;"
            />
            <div
                class="absolute glow-drift"
                style="top: 64%; left: 28%; width: 240px; height: 240px;
                       background: radial-gradient(circle closest-side at center,
                         rgba(52,211,153,0.45) 0%, rgba(52,211,153,0.15) 40%, transparent 75%);
                       filter: blur(60px); animation-delay: -9s;"
            />

            // 12 warping symbols across 8 exit directions, staggered so
            // the user always sees fresh things emerging from center.
            <WarpSymbol class_str="absolute warp-n sym-warm" size="88px" delay="-2s">
                <circle cx="12" cy="12" r="10" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-ne sym-sky" size="78px" delay="-5s">
                <path d="M12 3 21 20H3z" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-e sym-rose" size="64px" delay="-8s">
                <path d="M12 5v14M5 12h14" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-se sym-orange" size="76px" delay="-11s">
                <path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.5 5.5l2.8 2.8M15.7 15.7l2.8 2.8M5.5 18.5l2.8-2.8M15.7 8.3l2.8-2.8" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-s sym-emerald" size="84px" delay="-14s">
                <rect x="3" y="3" width="18" height="18" rx="3" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-sw sym-violet" size="100px" delay="-17s">
                <circle cx="12" cy="12" r="3" />
                <circle cx="12" cy="12" r="7" />
                <circle cx="12" cy="12" r="11" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-w sym-indigo" size="68px" delay="-20s">
                <path d="M12 2 22 7v10l-10 5L2 17V7z" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-nw sym-cyan" size="72px" delay="-23s">
                <rect x="6" y="12" width="12" height="9" rx="2" />
                <path d="M9 12V8a3 3 0 0 1 6 0v4" />
            </WarpSymbol>

            // Filled star — solid fill variation against the strokes.
            <svg
                class="absolute warp-n sym-lime"
                style="top: 50%; left: 50%; width: 64px; height: 64px; animation-delay: -9s;"
                viewBox="0 0 24 24" fill="currentColor"
            >
                <path d="M12 2 14.6 9.3 22 10l-5.8 4.9L18 22l-6-4-6 4 1.8-7.1L2 10l7.4-.7z" />
            </svg>

            <WarpSymbol class_str="absolute warp-ne sym-rose" size="58px" delay="-16s">
                <path d="M12 12c-4 0-4-6 0-6s4 6 0 6-4-9 0-9 8 9 0 9-9-12 0-12" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-sw sym-warm" size="60px" delay="-12s">
                <path d="M12 2 22 12 12 22 2 12z" />
            </WarpSymbol>
            <WarpSymbol class_str="absolute warp-e sym-violet" size="70px" delay="-19s">
                <circle cx="12" cy="12" r="8" />
                <path d="M12 4v4M12 16v4M4 12h4M16 12h4" />
            </WarpSymbol>
        </div>
    }
}

#[component]
fn WarpSymbol(
    #[prop(into)] class_str: String,
    #[prop(into)] size: String,
    #[prop(into)] delay: String,
    children: Children,
) -> impl IntoView {
    // All warp symbols start at the viewport center; the keyframe's
    // initial translate(-50%, -50%) re-centers them and the final
    // translate carries them past the screen edge.
    let style = format!(
        "top: 50%; left: 50%; width: {size}; height: {size}; animation-delay: {delay};"
    );
    view! {
        <svg
            class=class_str
            style=style
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.25"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            {children()}
        </svg>
    }
}
