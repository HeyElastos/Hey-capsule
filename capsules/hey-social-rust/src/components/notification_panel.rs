// NotificationPanel — dropdown panel showing recent notifications.
// Shown when the bell icon in TopHeader is clicked. Marks all as read on
// open. Each notification has a context-aware label (follow.request,
// post.react, etc.) and a "Dismiss" affordance.

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::notifications::{self, Notification};
use crate::components::NavLink;

#[component]
pub fn NotificationPanel(open: RwSignal<bool>) -> impl IntoView {
    let notes: RwSignal<Vec<Notification>> = RwSignal::new(Vec::new());

    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        spawn_local(async move {
            let list = notifications::list().await;
            notes.set(list);
            let _ = notifications::mark_all_read().await;
        });
    });

    view! {
        {move || if open.get() {
            view! {
                <div class="fixed inset-0 z-40 flex items-start justify-end p-4 pt-20" on:click=move |_: MouseEvent| open.set(false)>
                    <div
                        class="frosted-card w-80 max-h-[60vh] overflow-y-auto p-4"
                        on:click=|ev: MouseEvent| ev.stop_propagation()
                    >
                        <header class="flex items-baseline justify-between mb-3">
                            <h3 class="logo-handwritten text-2xl text-primary">"Notifications"</h3>
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
                            let list = notes.get();
                            if list.is_empty() {
                                view! {
                                    <p class="text-sm text-muted text-center py-8">"No notifications yet."</p>
                                }.into_any()
                            } else {
                                view! {
                                    <ul class="space-y-2">
                                        {list.into_iter().map(|n| view! { <NotificationRow n=n on_delete=move |id: String| {
                                            spawn_local(async move {
                                                let _ = notifications::delete(&id).await;
                                                notes.update(|ns| ns.retain(|x| x.id != id));
                                            });
                                        } /> }).collect::<Vec<_>>()}
                                    </ul>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
            }.into_any()
        } else { view! { <></> }.into_any() }}
    }
}

#[component]
fn NotificationRow(
    n: Notification,
    on_delete: impl Fn(String) + 'static + Send + Sync + Clone,
) -> impl IntoView {
    let label = match n.event_type.as_str() {
        "follow.request" => format!(
            "{} started following you",
            if n.from_name.is_empty() {
                short_did(&n.from_did)
            } else {
                n.from_name.clone()
            }
        ),
        "post.react" => format!(
            "{} reacted {}",
            if n.from_name.is_empty() {
                short_did(&n.from_did)
            } else {
                n.from_name.clone()
            },
            n.emoji.clone().unwrap_or_default()
        ),
        other => format!("{other}"),
    };
    let id = n.id.clone();
    let did_link = format!("/profile/{}", n.from_did);

    view! {
        <li class="rounded-2xl bg-white/10 border border-surface p-3 flex items-start gap-2">
            <NavLink href=did_link class="flex-1 text-sm text-primary hover:underline">{label}</NavLink>
            <button
                type="button"
                on:click=move |_| on_delete(id.clone())
                class="icon-btn-ghost"
                aria-label="Dismiss"
                title="Dismiss"
            >
                <svg viewBox="0 0 24 24" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M18 6 6 18M6 6l12 12" />
                </svg>
            </button>
        </li>
    }
}

fn short_did(did: &str) -> String {
    if did.len() > 18 {
        format!("{}…", did.chars().take(18).collect::<String>())
    } else {
        did.into()
    }
}
