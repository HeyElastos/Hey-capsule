use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

use crate::passkey::{passkey_supported, sign_in_via_runtime};
use crate::session;

#[component]
pub fn Landing() -> impl IntoView {
    // Splash → "Sign in" CTA. Mirrors hey-social/client/src/pages/Landing.jsx
    // (trimmed; no FloatingScene SVG yet in the Rust port).
    let navigate = use_navigate();
    let go_signin = move |_| {
        navigate("/signin", NavigateOptions::default());
    };
    view! {
        <section class="min-h-screen flex items-center justify-center px-4 py-8 bg-gradient-to-br from-amber-50 via-rose-50 to-slate-100 dark:from-slate-950 dark:via-slate-950 dark:to-slate-900">
            <div class="max-w-md w-full text-center">
                <h1 class="text-5xl font-bold tracking-tight text-slate-900 dark:text-slate-50">
                    "Hey " <span class="text-amber-500">"Social"</span>
                </h1>
                <p class="mt-3 text-sm text-slate-600 dark:text-slate-400">
                    "Photos, video, chat on Elastos. Federated peer-to-peer. End-to-end encrypted."
                </p>
                <button
                    type="button"
                    on:click=go_signin
                    class="mt-8 inline-flex items-center justify-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-6 py-3.5 text-base shadow-lg transition-colors"
                >
                    "Get started"
                </button>
            </div>
        </section>
    }
}

#[component]
pub fn SignIn() -> impl IntoView {
    // Calls signInViaRuntime — the same upstream /api/auth/passkey/{begin,complete}
    // contract Hey Social and Hey Messenger use. On success we route to /home.
    let busy = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let navigate = use_navigate();
    let supported = passkey_supported();

    let handle_passkey = {
        let navigate = navigate.clone();
        move |_| {
            if busy.get() {
                return;
            }
            error.set(String::new());
            busy.set(true);
            let navigate = navigate.clone();
            spawn_local(async move {
                match sign_in_via_runtime(None).await {
                    Ok(_session) => {
                        busy.set(false);
                        navigate("/home", NavigateOptions::default());
                    }
                    Err(msg) => {
                        busy.set(false);
                        error.set(msg);
                    }
                }
            });
        }
    };

    view! {
        <section class="relative flex min-h-screen w-full items-center justify-center px-4 py-8 bg-gradient-to-br from-amber-50 via-rose-50 to-slate-100 dark:from-slate-950 dark:via-slate-950 dark:to-slate-900">
            <div aria-hidden="true" class="pointer-events-none absolute -top-32 -left-32 h-96 w-96 rounded-full bg-amber-400/20 blur-3xl dark:bg-amber-500/10" />
            <div aria-hidden="true" class="pointer-events-none absolute -bottom-32 -right-32 h-96 w-96 rounded-full bg-rose-400/20 blur-3xl dark:bg-rose-500/10" />
            <div class="relative z-10 w-full max-w-md">
                <div class="text-center mb-8">
                    <h1 class="text-5xl font-bold tracking-tight text-slate-900 dark:text-slate-50">
                        "Hey " <span class="text-amber-500">"Social"</span>
                    </h1>
                    <p class="mt-3 text-sm text-slate-600 dark:text-slate-400">
                        "Sign in with the same passkey you used in System."
                    </p>
                </div>
                <div class="rounded-2xl bg-white/80 dark:bg-slate-900/70 backdrop-blur-xl border border-slate-200/70 dark:border-slate-800/70 p-6 shadow-xl">
                    {move || if supported {
                        view! {
                            <>
                                <button
                                    type="button"
                                    on:click=handle_passkey.clone()
                                    prop:disabled=move || busy.get()
                                    class="w-full inline-flex items-center justify-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 disabled:bg-slate-300 dark:disabled:bg-slate-700 disabled:cursor-not-allowed text-white font-semibold px-6 py-3.5 text-base shadow-lg transition-colors"
                                >
                                    <svg viewBox="0 0 24 24" class="h-5 w-5 fill-current">
                                        <path d="M12 2a5 5 0 0 0-5 5v3H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8a2 2 0 0 0-2-2h-1V7a5 5 0 0 0-5-5Zm-3 8V7a3 3 0 0 1 6 0v3H9Z" />
                                    </svg>
                                    {move || if busy.get() { "Waiting for passkey…" } else { "Sign in with passkey" }}
                                </button>
                                <p class="mt-3 text-center text-[12px] text-slate-500 dark:text-slate-400">
                                    "Same passkey as System. One tap, nothing to remember."
                                </p>
                            </>
                        }.into_any()
                    } else {
                        view! {
                            <div class="text-sm text-slate-600 dark:text-slate-400 text-center">
                                "Your browser doesn't support passkeys. Use a modern Chrome / Edge / Safari / Firefox to sign in."
                            </div>
                        }.into_any()
                    }}
                    {move || {
                        let msg = error.get();
                        if msg.is_empty() {
                            view! { <></> }.into_any()
                        } else {
                            view! {
                                <p class="mt-4 text-sm text-red-500 dark:text-red-400 text-center">{msg}</p>
                            }.into_any()
                        }
                    }}
                </div>
            </div>
        </section>
    }
}

#[component]
pub fn SignUp() -> impl IntoView {
    // Sign-up runs in the home shell (System) — the Rust port re-uses that
    // contract via the same passkey on /signin. Keep the route for parity.
    view! { <Stub label="Sign Up (handled in System)" /> }
}

#[component]
pub fn Onboarding() -> impl IntoView {
    view! { <Stub label="Onboarding" /> }
}

#[component]
pub fn Home() -> impl IntoView {
    // Post-signin landing. Empty-state today; the React reference's feed
    // (capsules/hey-social/client/src/pages/Home.jsx) is the next thing
    // to port — provider/peer/ipfs wiring comes online before that's
    // possible. For now we render the signed-in identity so the user
    // can confirm the cross-capsule DID matches what other Hey capsules
    // and the home shell report.
    let user = session::current();
    let navigate = use_navigate();
    let sign_out = move |_| {
        session::clear();
        navigate("/signin", NavigateOptions::default());
    };
    view! {
        <AppShell>
            {match user {
                Some(s) => view! {
                    <div class="max-w-2xl mx-auto px-4 py-10">
                        <div class="rounded-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 p-6 shadow-sm">
                            <div class="flex items-start justify-between gap-4">
                                <div>
                                    <p class="text-[11px] uppercase tracking-wider text-emerald-600 dark:text-emerald-400">
                                        "Signed in"
                                    </p>
                                    <h2 class="mt-1 text-xl font-semibold text-slate-900 dark:text-slate-50">
                                        {s.name.clone()}
                                    </h2>
                                    <p class="mt-1 text-[12px] font-mono text-slate-500 dark:text-slate-400 break-all">
                                        {s.did_key}
                                    </p>
                                </div>
                                <button
                                    type="button"
                                    on:click=sign_out
                                    class="shrink-0 text-xs font-medium text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200 underline-offset-4 hover:underline"
                                >
                                    "Sign out"
                                </button>
                            </div>
                        </div>

                        <div class="mt-8 text-center">
                            <div class="inline-flex h-16 w-16 items-center justify-center rounded-full bg-amber-100 dark:bg-amber-500/20 text-amber-600 dark:text-amber-300">
                                <svg viewBox="0 0 24 24" class="h-8 w-8 fill-current">
                                    <path d="M4 5h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Zm0 2v10h16V7H4Zm3 2 3 3 2-2 4 4H7l0-5Zm10 0a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z" />
                                </svg>
                            </div>
                            <h3 class="mt-4 text-lg font-semibold text-slate-900 dark:text-slate-50">
                                "Your feed is empty"
                            </h3>
                            <p class="mt-2 text-sm text-slate-600 dark:text-slate-400 max-w-sm mx-auto">
                                "Follow someone or share your first photo to start filling this space. The Rust port doesn't have the post + peer wiring yet — use Hey Social (React) for actual posting until those ports land."
                            </p>
                        </div>
                    </div>
                }.into_any(),
                None => view! {
                    <p class="px-4 py-10 text-center text-sm text-slate-500 dark:text-slate-400">
                        "Redirecting to sign in…"
                    </p>
                }.into_any(),
            }}
        </AppShell>
    }
}

#[component]
fn AppShell(children: Children) -> impl IntoView {
    // Shared chrome for post-signin pages: a thin sticky header with the
    // Hey wordmark. The full nav (feed / clips / chat / profile) lands
    // when those pages stop being stubs.
    view! {
        <div class="min-h-screen bg-slate-50 dark:bg-slate-950">
            <header class="sticky top-0 z-10 border-b border-slate-200 dark:border-slate-800 bg-white/80 dark:bg-slate-950/80 backdrop-blur">
                <div class="max-w-2xl mx-auto px-4 h-14 flex items-center">
                    <span class="text-lg font-semibold tracking-tight text-slate-900 dark:text-slate-50">
                        "Hey " <span class="text-amber-500">"Social"</span>
                    </span>
                </div>
            </header>
            {children()}
        </div>
    }
}

#[component]
pub fn Clips() -> impl IntoView {
    view! { <Stub label="Clips (video feed)" /> }
}

#[component]
pub fn PostDetail() -> impl IntoView {
    view! { <Stub label="Post detail" /> }
}

#[component]
pub fn VideoPlayer() -> impl IntoView {
    view! { <Stub label="Video player (Elacity-backed)" /> }
}

#[component]
pub fn Profile() -> impl IntoView {
    view! { <Stub label="Profile" /> }
}

#[component]
pub fn Chat() -> impl IntoView {
    view! { <Stub label="Chat" /> }
}

#[component]
pub fn NotFound() -> impl IntoView {
    view! { <Stub label="404" /> }
}

#[component]
fn Stub(label: &'static str) -> impl IntoView {
    view! {
        <section style="padding: 2rem; font-family: system-ui">
            <h1 style="font-size: 1.5rem; font-weight: 600">{label}</h1>
            <p style="opacity: 0.6">"Placeholder — port from capsules/hey-social/client/src/."</p>
        </section>
    }
}
