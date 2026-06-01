// Modal — shared shell for centered popups.
//
// All three app-level modals (NotificationPanel, SearchModal,
// AddFriendModal) use this so they get:
//   * Vertically + horizontally centered on every viewport
//   * Backdrop click closes
//   * Escape key closes (window keydown listener bound on open)
//   * Fade-in animation on mount
//
// Uses <Show> so the children closure is FnOnce-friendly per-open.

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

#[component]
pub fn Modal(open: RwSignal<bool>, children: ChildrenFn) -> impl IntoView {
    // Escape-to-close. Bind the window keydown listener ONCE for this
    // modal's lifetime. The handler uses disposal-safe `try_*` accessors:
    // after the modal's reactive owner is gone, `try_get_untracked` returns
    // None and the handler no-ops instead of crashing. (Previously an Effect
    // re-added + `.forget()`'d a fresh listener on every `open` toggle, and
    // those leaked closures called `open.set` on a disposed signal after
    // unmount → "closure invoked recursively or after being dropped".)
    if let Some(win) = web_sys::window() {
        let closure: Closure<dyn FnMut(KeyboardEvent)> =
            Closure::wrap(Box::new(move |ev: KeyboardEvent| {
                if ev.key() == "Escape" && open.try_get_untracked() == Some(true) {
                    let _ = open.try_set(false);
                }
            }));
        let _ =
            win.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    view! {
        <Show when=move || open.get() fallback=|| view! { <></> }>
            <div
                class="modal-anchor fixed inset-0 z-50 flex items-start justify-center bg-black/40 backdrop-blur-sm px-4 pb-4 animate-fade-in"
                on:click=move |_: MouseEvent| open.set(false)
            >
                <div
                    class="modal-reveal w-full max-w-md"
                    on:click=|ev: MouseEvent| ev.stop_propagation()
                >
                    {children()}
                </div>
            </div>
        </Show>
    }
}
