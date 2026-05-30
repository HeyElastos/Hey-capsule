use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, HtmlInputElement};

use crate::api::profile::{ensure_profile, update_profile, upload_avatar, ProfileUpdate};
use crate::session;

#[component]
pub fn Onboarding() -> impl IntoView {
    let navigate = use_navigate();
    let leaving = RwSignal::new(false);

    // Profile-setup form state.
    let name = RwSignal::new(String::new());
    let bio = RwSignal::new(String::new());
    let avatar_url = RwSignal::new(String::new());
    let avatar_busy = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    // First-visit gate. The welcome screen is a one-shot intro — once
    // the user has seen it, returning sessions (passkey re-login, direct
    // /welcome URL, back button) bounce straight to the feed. We mark the
    // welcomed flag only when the user finishes (Save & continue / Skip),
    // NOT on mount, so a reload mid-setup still shows the form. The
    // already-welcomed branch bounces straight to the feed.
    Effect::new({
        let navigate = navigate.clone();
        move |_| {
            if session::welcomed() {
                navigate("/home", NavigateOptions::default());
            }
        }
    });

    // Prefill the form from the current profile/session so the
    // `hey-XXXXXX` placeholder name is editable rather than blank.
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(me) = ensure_profile().await {
                name.set(me.name);
                bio.set(me.bio);
                avatar_url.set(me.avatar);
            } else if let Some(s) = session::current() {
                name.set(s.name);
            }
        });
    });

    // Warp out to the feed using the same transition the page already
    // used: flip `leaving` (drives .warp-transition on the section), wait
    // a full keyframe, then navigate so the feed's warp-in is seamless.
    // Cloneable so both "Save & continue" and "Skip" can own a copy.
    let warp_to_feed = {
        let navigate = navigate.clone();
        move || {
            if leaving.get() {
                return;
            }
            session::mark_welcomed();
            leaving.set(true);
            let navigate = navigate.clone();
            spawn_local(async move {
                wait_ms(1000).await;
                navigate("/", NavigateOptions::default());
            });
        }
    };

    let on_name_input = move |ev: Event| {
        if let Some(t) = ev.target() {
            if let Ok(i) = t.dyn_into::<HtmlInputElement>() {
                name.set(i.value());
            }
        }
    };
    let on_bio_input = move |ev: Event| {
        if let Some(t) = ev.target() {
            if let Ok(ta) = t.dyn_into::<web_sys::HtmlTextAreaElement>() {
                bio.set(ta.value());
            }
        }
    };

    // Avatar pick — mirrors profile.rs: read the file's bytes, then
    // upload_avatar() compresses (WebP) + pins to IPFS + writes the new
    // gateway URL into profile.json. We reflect the returned URL in the
    // round preview.
    let on_avatar_change = move |ev: Event| {
        let Some(target) = ev.target() else { return };
        let Ok(input) = target.dyn_into::<HtmlInputElement>() else { return };
        let Some(files) = input.files() else { return };
        if files.length() == 0 {
            return;
        }
        let Some(file) = files.get(0) else { return };
        let fname = file.name();
        let mime = file.type_();
        error.set(String::new());
        avatar_busy.set(true);
        spawn_local(async move {
            let buf_value = match JsFuture::from(file.array_buffer()).await {
                Ok(v) => v,
                Err(_) => {
                    error.set("Couldn't read that image.".into());
                    avatar_busy.set(false);
                    return;
                }
            };
            let array = js_sys::Uint8Array::new(&buf_value);
            let mut bytes = vec![0u8; array.length() as usize];
            array.copy_to(&mut bytes);
            match upload_avatar(&bytes, &fname, &mime).await {
                Ok(p) => avatar_url.set(p.avatar),
                Err(e) => error.set(format!("{e}")),
            }
            avatar_busy.set(false);
        });
    };

    // Save & continue — persist name + bio (avatar is already written by
    // upload_avatar on pick), then warp to the feed.
    let save_and_continue = {
        let warp_to_feed = warp_to_feed.clone();
        move |_| {
            if saving.get() || leaving.get() || avatar_busy.get() {
                return;
            }
            let n = name.get();
            let b = bio.get();
            saving.set(true);
            error.set(String::new());
            let warp_to_feed = warp_to_feed.clone();
            spawn_local(async move {
                match update_profile(ProfileUpdate {
                    name: Some(n),
                    bio: Some(b),
                    avatar: None,
                })
                .await
                {
                    Ok(_) => {
                        saving.set(false);
                        warp_to_feed();
                    }
                    Err(e) => {
                        error.set(format!("{e}"));
                        saving.set(false);
                    }
                }
            });
        }
    };

    let skip = move |_| {
        if saving.get() || leaving.get() {
            return;
        }
        warp_to_feed();
    };

    let busy = move || saving.get() || avatar_busy.get() || leaving.get();

    view! {
        <section
            class="warp-in relative min-h-screen flex items-center justify-center pl-24 pr-3 py-6 sm:pl-28 sm:pr-6 sm:py-10 overflow-hidden"
            class:warp-transition=move || leaving.get()
        >
            <OnboardingScene />
            <div class="relative z-10 w-full max-w-xl">
                <div class="frosted-card frosted-card-strong p-8 sm:p-10 animate-fade-up">
                    <h1 class="logo-handwritten text-5xl sm:text-6xl text-primary leading-tight text-center">
                        "Set up your profile"
                    </h1>
                    <p class="mt-3 text-sm text-muted text-center max-w-md mx-auto leading-6">
                        "Pick a name, add a photo, and say a little about yourself. You can change all of this later."
                    </p>

                    // Avatar picker — round preview + hidden file input.
                    <div class="mt-8 flex justify-center">
                        <label class="relative h-24 w-24 cursor-pointer">
                            {move || {
                                let url = avatar_url.get();
                                if url.is_empty() {
                                    let initial = name
                                        .get()
                                        .chars()
                                        .next()
                                        .map(|c| c.to_uppercase().next().unwrap_or(c).to_string())
                                        .unwrap_or_else(|| "?".into());
                                    view! {
                                        <div class="h-24 w-24 rounded-full bg-gradient-to-br from-accent to-amber-600 grid place-items-center text-accent-text text-3xl font-bold shadow-sm">
                                            {initial}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <img
                                            src=url
                                            alt=""
                                            class="h-24 w-24 rounded-full object-cover ring-1 ring-black/10 dark:ring-white/15 shadow-sm"
                                        />
                                    }.into_any()
                                }
                            }}
                            <input
                                type="file"
                                class="sr-only"
                                accept="image/*"
                                prop:disabled=move || busy()
                                on:change=on_avatar_change
                            />
                            <span class="absolute -bottom-1 -right-1 inline-flex h-8 w-8 items-center justify-center rounded-full bg-accent text-accent-text shadow-md">
                                {move || if avatar_busy.get() {
                                    view! {
                                        <svg viewBox="0 0 24 24" class="spinner h-4 w-4" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true">
                                            <path d="M21 12a9 9 0 1 1-6.2-8.5" />
                                        </svg>
                                    }.into_any()
                                } else {
                                    view! {
                                        <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                            <path d="M12 5v14M5 12h14" />
                                        </svg>
                                    }.into_any()
                                }}
                            </span>
                        </label>
                    </div>

                    // Nickname.
                    <div class="mt-7 space-y-1.5">
                        <label class="block text-xs font-medium uppercase tracking-wider text-muted">
                            "Nickname"
                        </label>
                        <input
                            type="text"
                            class="frosted-input text-sm"
                            maxlength="30"
                            placeholder="Your display name"
                            prop:value=move || name.get()
                            on:input=on_name_input
                        />
                    </div>

                    // Bio.
                    <div class="mt-4 space-y-1.5">
                        <label class="block text-xs font-medium uppercase tracking-wider text-muted">
                            "Bio"
                        </label>
                        <textarea
                            class="frosted-input text-sm"
                            rows="3"
                            maxlength="280"
                            placeholder="A short bio (optional)"
                            prop:value=move || bio.get()
                            on:input=on_bio_input
                        />
                    </div>

                    {move || {
                        let m = error.get();
                        if m.is_empty() { view! { <></> }.into_any() }
                        else { view! { <p class="mt-3 text-xs text-rose-500">{m}</p> }.into_any() }
                    }}

                    <button
                        type="button"
                        on:click=save_and_continue
                        prop:disabled=busy
                        class="unfrost mt-7 inline-flex w-full items-center justify-center gap-2 rounded-full bg-accent px-7 py-3 text-base font-semibold text-accent-text shadow-md transition hover:bg-amber-300 disabled:opacity-60 disabled:cursor-not-allowed"
                    >
                        {move || if saving.get() || leaving.get() {
                            view! {
                                <svg viewBox="0 0 24 24" class="spinner h-4 w-4" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true">
                                    <path d="M21 12a9 9 0 1 1-6.2-8.5" />
                                </svg>
                                "Saving…"
                            }.into_any()
                        } else {
                            view! {
                                "Save & continue"
                                <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                    <path d="M5 12h14M13 5l7 7-7 7" />
                                </svg>
                            }.into_any()
                        }}
                    </button>

                    <div class="mt-3 text-center">
                        <button
                            type="button"
                            on:click=skip
                            prop:disabled=move || saving.get() || leaving.get()
                            class="text-xs font-medium text-muted underline-offset-2 hover:underline hover:text-primary transition-colors disabled:opacity-60"
                        >
                            "Skip for now"
                        </button>
                    </div>
                </div>
            </div>
        </section>
    }
}

// Background scene: symbols gently drift in place, never leaving their
// patch of the viewport. The drama (warp) is reserved for the one-shot
// page transition when the user taps "Go to feed" — see the
// .warp-transition class applied to the section root.
//
// Each symbol gets a position + a drift-* keyframe + a staggered
// animation-delay so nothing syncs.
#[component]
fn OnboardingScene() -> impl IntoView {
    view! {
        <div class="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
            // Slow-drifting gradient blobs anchor the scene.
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

            // Drifting symbols — each pinned to its own corner of the
            // viewport, gently swaying in place.
            <DriftSymbol drift="drift-a" color="sym-warm"    pos="top: 12%; left: 14%; width: 88px; height: 88px;" delay="-1s">
                <circle cx="12" cy="12" r="10" />
            </DriftSymbol>
            <DriftSymbol drift="drift-b" color="sym-sky"     pos="top: 22%; left: 18%; width: 74px; height: 74px;" delay="-5s">
                <path d="M12 3 21 20H3z" />
            </DriftSymbol>
            <DriftSymbol drift="drift-c" color="sym-rose"    pos="bottom: 22%; left: 10%; width: 62px; height: 62px;" delay="-8s">
                <path d="M12 5v14M5 12h14" />
            </DriftSymbol>
            <DriftSymbol drift="drift-d" color="sym-orange"  pos="top: 28%; left: 56%; width: 72px; height: 72px;" delay="-3s">
                <path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.5 5.5l2.8 2.8M15.7 15.7l2.8 2.8M5.5 18.5l2.8-2.8M15.7 8.3l2.8-2.8" />
            </DriftSymbol>
            <DriftSymbol drift="drift-a" color="sym-emerald" pos="top: 58%; right: 16%; width: 84px; height: 84px;" delay="-12s">
                <rect x="3" y="3" width="18" height="18" rx="3" />
            </DriftSymbol>
            <DriftSymbol drift="drift-b" color="sym-violet"  pos="bottom: 32%; right: 30%; width: 108px; height: 108px;" delay="-7s">
                <circle cx="12" cy="12" r="3" />
                <circle cx="12" cy="12" r="7" />
                <circle cx="12" cy="12" r="11" />
            </DriftSymbol>
            <DriftSymbol drift="drift-c" color="sym-indigo"  pos="top: 6%; left: 44%; width: 66px; height: 66px;" delay="-10s">
                <path d="M12 2 22 7v10l-10 5L2 17V7z" />
            </DriftSymbol>
            <DriftSymbol drift="drift-d" color="sym-cyan"    pos="top: 48%; left: 6%; width: 80px; height: 80px;" delay="-15s">
                <rect x="6" y="12" width="12" height="9" rx="2" />
                <path d="M9 12V8a3 3 0 0 1 6 0v4" />
            </DriftSymbol>

            // Filled star — solid fill for variety.
            <svg
                class="absolute drift-a sym-lime"
                style="bottom: 18%; left: 60%; width: 60px; height: 60px; animation-delay: -6s;"
                viewBox="0 0 24 24" fill="currentColor"
            >
                <path d="M12 2 14.6 9.3 22 10l-5.8 4.9L18 22l-6-4-6 4 1.8-7.1L2 10l7.4-.7z" />
            </svg>

            <DriftSymbol drift="drift-c" color="sym-rose"   pos="top: 72%; left: 38%; width: 56px; height: 56px;" delay="-14s">
                <path d="M12 12c-4 0-4-6 0-6s4 6 0 6-4-9 0-9 8 9 0 9-9-12 0-12" />
            </DriftSymbol>
            <DriftSymbol drift="drift-b" color="sym-warm"   pos="top: 34%; right: 8%; width: 58px; height: 58px;" delay="-4s">
                <path d="M12 2 22 12 12 22 2 12z" />
            </DriftSymbol>
        </div>
    }
}

#[component]
fn DriftSymbol(
    #[prop(into)] drift: String,
    #[prop(into)] color: String,
    #[prop(into)] pos: String,
    #[prop(into)] delay: String,
    children: Children,
) -> impl IntoView {
    let class_str = format!("absolute {drift} {color}");
    let style = format!("{pos} animation-delay: {delay};");
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

async fn wait_ms(ms: i32) {
    let win = web_sys::window().unwrap();
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let _ = win
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}
