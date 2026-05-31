// ContactsPanel — centered popup listing the people you chat with (your
// DM contacts), openable from the FloatingDock so you can jump into a
// conversation from any page. Uses the shared <Modal> shell for centering
// + Esc + backdrop-close + fade-in.
//
// Each row links to /chat/<did>; clicking a row navigates AND closes the
// panel (the click bubbles to the <li>, which flips `open` to false).
// Mirrors NotificationPanel's structure and the chat page's contact rows.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::dms::{list_contacts, DmContact};
use crate::app_modals::AppModals;
use crate::components::icons::UsersIcon;
use crate::components::{Modal, NavLink};

#[component]
pub fn ContactsPanel(open: RwSignal<bool>) -> impl IntoView {
    let contacts: RwSignal<Vec<DmContact>> = RwSignal::new(Vec::new());
    let loaded = RwSignal::new(false);
    let modals = use_context::<AppModals>().unwrap_or_default();

    // (Re)load the contact list every time the panel opens so it reflects
    // any new conversations started elsewhere.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        loaded.set(false);
        spawn_local(async move {
            let list = list_contacts().await;
            contacts.set(list);
            loaded.set(true);
        });
    });

    view! {
        <Modal open=open>
            <div class="frosted-card frosted-card-strong p-5 max-h-[70vh] overflow-y-auto">
                <header class="flex items-baseline justify-between mb-3">
                    <h3 class="logo-handwritten text-4xl text-primary">"Contacts"</h3>
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
                {move || {
                    let list = contacts.get();
                    if list.is_empty() {
                        // Distinguish "still loading" from "genuinely empty" so
                        // the panel doesn't flash the empty hint before the
                        // first fetch resolves.
                        if !loaded.get() {
                            view! { <></> }.into_any()
                        } else {
                            view! {
                                <div class="text-center py-10">
                                    <div class="float-soft inline-flex h-14 w-14 items-center justify-center rounded-2xl border border-white/20 bg-white/10 backdrop-blur-xl text-accent">
                                        <UsersIcon class="h-6 w-6" />
                                    </div>
                                    <p class="mt-4 text-sm text-muted">"No contacts yet."</p>
                                    <button
                                        type="button"
                                        on:click=move |_| {
                                            open.set(false);
                                            modals.add_friend_open.set(true);
                                        }
                                        class="unfrost mt-4 inline-flex items-center gap-2 rounded-full bg-accent px-5 py-2 text-xs font-semibold text-accent-text hover:bg-amber-300 transition-colors"
                                    >
                                        "Add a friend"
                                    </button>
                                </div>
                            }.into_any()
                        }
                    } else {
                        view! {
                            <ul class="space-y-1.5">
                                {list.into_iter().map(|c| view! { <ContactRow c=c on_navigate=move || open.set(false) /> }).collect::<Vec<_>>()}
                            </ul>
                        }.into_any()
                    }
                }}
            </div>
        </Modal>
    }
}

#[component]
fn ContactRow(
    c: DmContact,
    on_navigate: impl Fn() + 'static + Send + Sync + Clone,
) -> impl IntoView {
    let name = display_name(&c);
    let preview = c.last_preview.clone();
    let ts = ts_short(c.last_ts);
    let unread = c.unread;
    let href = format!("/chat/{}", c.did);
    let avatar = avatar_letters(&c);

    view! {
        // The click bubbles from the inner NavLink up to this <li>, so a tap
        // both navigates (NavLink) and closes the panel (on_navigate).
        <li on:click=move |_| on_navigate()>
            <NavLink
                href=href
                class="flex items-center gap-3 rounded-2xl px-3 py-2.5 hover:bg-white/10 transition-colors"
            >
                <span class="flex h-10 w-10 flex-none items-center justify-center rounded-full bg-gradient-to-br from-accent to-amber-600 text-accent-text text-sm font-bold shadow-sm">
                    {avatar}
                </span>
                <div class="flex-1 min-w-0">
                    <div class="flex items-baseline justify-between gap-2">
                        <span class="text-sm font-medium text-primary truncate">{name}</span>
                        <span class="text-[10px] text-muted shrink-0">{ts}</span>
                    </div>
                    <div class="flex items-center justify-between gap-2">
                        <p class="text-xs text-muted truncate">{preview}</p>
                        {if unread > 0 {
                            view! {
                                <span class="inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-accent text-accent-text text-[10px] font-bold px-1.5 shrink-0">
                                    {if unread > 9 { "9+".to_string() } else { unread.to_string() }}
                                </span>
                            }.into_any()
                        } else { view! { <></> }.into_any() }}
                    </div>
                </div>
            </NavLink>
        </li>
    }
}

fn display_name(c: &DmContact) -> String {
    if !c.name.is_empty() {
        return c.name.clone();
    }
    if c.did.starts_with("pending:") {
        return "Awaiting reply…".into();
    }
    short_did(&c.did)
}

fn avatar_letters(c: &DmContact) -> String {
    if !c.name.is_empty() {
        return initial_letters(&c.name);
    }
    short_did(&c.did).chars().take(2).collect::<String>().to_uppercase()
}

fn initial_letters(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect::<String>()
        .to_uppercase()
}

fn short_did(did: &str) -> String {
    if did.starts_with("pending:") {
        return "(invite pending)".into();
    }
    let s = did.strip_prefix("did:key:z").unwrap_or(did);
    if s.len() > 12 {
        format!("{}…", s.chars().take(12).collect::<String>())
    } else {
        s.into()
    }
}

fn ts_short(ts: i64) -> String {
    if ts == 0 {
        return String::new();
    }
    let now = js_sys::Date::now();
    let diff_secs = ((now - ts as f64) / 1000.0).max(0.0) as i64;
    if diff_secs < 60 {
        return "now".into();
    }
    let mins = diff_secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts as f64));
    d.to_locale_date_string("en-US", &wasm_bindgen::JsValue::UNDEFINED)
        .as_string()
        .unwrap_or_default()
}
