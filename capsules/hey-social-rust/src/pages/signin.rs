// SignIn — passkey ceremony entry point. Calls signInViaRuntime (the same
// upstream contract Hey Social + Hey Messenger use); on success routes to
// /home. Mirrors capsules/hey-social/client/src/pages/Landing.jsx's
// "passkey" CTA + error handling.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

use crate::passkey::{passkey_supported, sign_in_via_runtime};

#[component]
pub fn SignIn() -> impl IntoView {
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
