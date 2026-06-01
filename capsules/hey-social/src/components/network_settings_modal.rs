// NetworkSettingsModal — peer-provider "Network / P2P" settings.
//
// Lets the user choose how their runtime federates over elastos://peer/*:
//   * Zero-config (default): iroh n0 relay + discovery — works behind NAT,
//     leans on n0's public relays.
//   * Independent: no third-party relay; peers reach this node ONLY via the
//     direct addresses in the shareable ticket. Needs a fixed UDP port +
//     a public address. Zero outside dependency.
// Also surfaces the node id + shareable ticket. Calls the shared hey-core
// helpers (hey_core::runtime::peer::{get_config,set_config}) so hey-chat can
// carry an identical panel against the same runtime node.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::components::Modal;

#[component]
pub fn NetworkSettingsModal(open: RwSignal<bool>) -> impl IntoView {
    let loaded = RwSignal::new(false);
    let node_id = RwSignal::new(String::new());
    let ticket = RwSignal::new(String::new());
    let independent = RwSignal::new(false);
    let bind_port = RwSignal::new(String::new());
    let public_addr = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let msg = RwSignal::new(String::new());
    let copied = RwSignal::new(false);

    // Load current config whenever the modal opens.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        loaded.set(false);
        spawn_local(async move {
            if let Some(c) = hey_core::runtime::peer::get_config().await {
                node_id.set(c.node_id);
                ticket.set(c.ticket);
                independent.set(c.relay_mode == "independent" || c.running_independent);
                bind_port.set(if c.bind_port == 0 { String::new() } else { c.bind_port.to_string() });
                public_addr.set(c.public_addr);
            }
            loaded.set(true);
        });
    });

    // Handlers defined OUTSIDE view! (turbofish in view! breaks the macro).
    let on_indep = move |ev: web_sys::Event| {
        if let Some(t) = ev.target() {
            if let Ok(i) = t.dyn_into::<HtmlInputElement>() {
                independent.set(i.checked());
            }
        }
    };
    let on_port = move |ev: web_sys::Event| {
        if let Some(t) = ev.target() {
            if let Ok(i) = t.dyn_into::<HtmlInputElement>() {
                bind_port.set(i.value());
            }
        }
    };
    let on_pub = move |ev: web_sys::Event| {
        if let Some(t) = ev.target() {
            if let Ok(i) = t.dyn_into::<HtmlInputElement>() {
                public_addr.set(i.value());
            }
        }
    };
    let save = move |_| {
        if busy.get() {
            return;
        }
        busy.set(true);
        msg.set(String::new());
        let mode = if independent.get() { "independent" } else { "default" };
        let port: u32 = bind_port.get().trim().parse().unwrap_or(0);
        let pub_a = public_addr.get().trim().to_string();
        spawn_local(async move {
            match hey_core::runtime::peer::set_config(mode, port, &pub_a).await {
                Ok(true) => msg.set("Saved — restart the runtime to apply the mode/port change.".into()),
                Ok(false) => msg.set("Saved.".into()),
                Err(e) => msg.set(format!("Error: {e}")),
            }
            busy.set(false);
        });
    };
    let copy_ticket = move |_| {
        let t = ticket.get();
        if t.is_empty() {
            return;
        }
        if let Some(w) = web_sys::window() {
            let _ = w.navigator().clipboard().write_text(&t);
        }
        copied.set(true);
        spawn_local(async move {
            crate::runtime::sleep_ms(1400).await;
            copied.set(false);
        });
    };

    view! {
        <Modal open=open>
            <div class="frosted-card frosted-card-strong p-5 space-y-3 text-left">
                <header class="flex items-center justify-between">
                    <h3 class="text-lg font-bold text-primary">"Network / P2P"</h3>
                    <button
                        type="button"
                        on:click=move |_| open.set(false)
                        class="icon-btn-ghost"
                        aria-label="Close"
                    >
                        <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
                    </button>
                </header>

                {move || if !loaded.get() {
                    view! { <p class="text-sm text-muted">"Loading…"</p> }.into_any()
                } else if node_id.get().is_empty() {
                    view! { <p class="text-sm text-amber-500 dark:text-amber-400">"Peer node not available on this runtime (same-runtime delivery only). Update the runtime to enable P2P."</p> }.into_any()
                } else {
                    let on_indep = on_indep.clone();
                    let on_port = on_port.clone();
                    let on_pub = on_pub.clone();
                    let save = save.clone();
                    let copy_ticket = copy_ticket.clone();
                    view! {
                        <div class="space-y-3">
                            <p class="text-xs text-muted break-all">
                                <span class="font-semibold text-primary">"Node: "</span>{move || node_id.get()}
                            </p>

                            <label class="flex items-center gap-2 text-sm text-primary cursor-pointer">
                                <input type="checkbox" prop:checked=move || independent.get() on:change=on_indep />
                                "Independent mode (direct, no relay)"
                            </label>
                            <p class="text-xs text-muted">"Off = zero-config (relayed, works behind NAT). On = no third-party relay — set a fixed port and a public address your peers can reach."</p>

                            <div>
                                <label class="text-xs font-semibold text-primary">"Fixed UDP port (blank = automatic)"</label>
                                <input type="number" class="frosted-input text-sm" placeholder="e.g. 4789" prop:value=move || bind_port.get() on:input=on_port />
                            </div>
                            <div>
                                <label class="text-xs font-semibold text-primary">"Public address (host:port) advertised in your ticket"</label>
                                <input type="text" class="frosted-input text-sm" placeholder="your-domain:4789" prop:value=move || public_addr.get() on:input=on_pub />
                            </div>

                            <div class="text-xs text-muted">
                                <div class="flex items-center justify-between">
                                    <span class="font-semibold text-primary">"Your node ticket"</span>
                                    <button type="button" on:click=copy_ticket class="text-accent hover:underline">{move || if copied.get() { "Copied" } else { "Copy" }}</button>
                                </div>
                                <p class="break-all opacity-70 max-h-16 overflow-y-auto">{move || ticket.get()}</p>
                            </div>

                            {move || { let m = msg.get(); if m.is_empty() { view! { <></> }.into_any() } else { view! { <p class="text-xs text-emerald-500 dark:text-emerald-400">{m}</p> }.into_any() } }}

                            <button
                                type="button"
                                on:click=save
                                prop:disabled=move || busy.get()
                                class="unfrost w-full rounded-full bg-accent hover:bg-amber-300 disabled:opacity-50 disabled:cursor-not-allowed text-accent-text font-semibold px-4 py-2.5 text-sm transition-colors"
                            >
                                {move || if busy.get() { "Saving…" } else { "Save" }}
                            </button>
                        </div>
                    }.into_any()
                }}
            </div>
        </Modal>
    }
}
