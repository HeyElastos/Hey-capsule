// SearchModal — paste-or-type-a-DID to navigate to a profile.
//
// The React reference does a richer search across cached peers + DID
// resolution. The Rust port starts with the simpler "type / paste a
// did:key:z..." path — covers ~80% of the use cases (adding a friend
// by their share-card or QR code).

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::components::icons::SearchIcon;

#[component]
pub fn SearchModal(open: RwSignal<bool>) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let navigate = use_navigate();

    let on_input = move |ev: web_sys::Event| {
        if let Some(t) = ev.target() {
            if let Ok(i) = t.dyn_into::<HtmlInputElement>() {
                query.set(i.value());
            }
        }
    };

    let on_submit = {
        let navigate = navigate.clone();
        move |_| {
            let q = query.get().trim().to_string();
            if !q.starts_with("did:key:z") {
                error.set("Enter a did:key:z… identity.".into());
                return;
            }
            error.set(String::new());
            open.set(false);
            navigate(&format!("/profile/{q}"), NavigateOptions::default());
        }
    };

    view! {
        {move || if open.get() {
            view! {
                <div class="fixed inset-0 z-40 flex items-start justify-center bg-black/40 p-4 pt-20" on:click=move |_: MouseEvent| open.set(false)>
                    <div
                        class="frosted-card w-full max-w-md p-4 space-y-3"
                        on:click=|ev: MouseEvent| ev.stop_propagation()
                    >
                        <header class="flex items-center justify-between">
                            <h3 class="logo-handwritten text-2xl text-primary">"Find someone"</h3>
                            <button
                                type="button"
                                on:click=move |_| open.set(false)
                                class="icon-btn-ghost"
                                aria-label="Close"
                            >
                                <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <path d="M18 6 6 18M6 6l12 12" />
                                </svg>
                            </button>
                        </header>
                        <div class="flex items-center gap-2">
                            <SearchIcon class="h-5 w-5 text-muted" />
                            <input
                                type="text"
                                class="frosted-input text-sm"
                                placeholder="did:key:z…"
                                prop:value=move || query.get()
                                on:input=on_input
                            />
                        </div>
                        {move || {
                            let m = error.get();
                            if m.is_empty() { view! { <></> }.into_any() }
                            else { view! { <p class="text-xs text-red-400">{m}</p> }.into_any() }
                        }}
                        <button
                            type="button"
                            on:click=on_submit.clone()
                            class="unfrost w-full rounded-full bg-accent hover:bg-amber-300 text-accent-text font-semibold px-4 py-2 text-sm"
                        >
                            "Open profile"
                        </button>
                    </div>
                </div>
            }.into_any()
        } else { view! { <></> }.into_any() }}
    }
}
