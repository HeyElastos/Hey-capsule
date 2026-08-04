//! Activity — what happened while you were elsewhere.
//!
//! Reads `hey_social::api::notifications`, the same store hey-social writes, so
//! marking something read here is read there too. One inbox, not two views of
//! two lists that drift.

use hey_social::api::notifications;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone, PartialEq)]
struct Item {
    id: String,
    kind: String,
    who: String,
    ts: i64,
    read: bool,
}

#[component]
pub fn Activity() -> impl IntoView {
    let (items, set_items) = signal(Vec::<Item>::new());
    let (loaded, set_loaded) = signal(false);

    let load = move || {
        spawn_local(async move {
            let ns = notifications::list().await;
            set_items.set(
                ns.iter()
                    .map(|n| Item {
                        id: n.id.clone(),
                        kind: n.event_type.clone(),
                        who: if n.from_name.is_empty() {
                            crate::shorten_did(&n.from_did)
                        } else {
                            n.from_name.clone()
                        },
                        ts: n.ts.unwrap_or(0),
                        read: n.read,
                    })
                    .collect(),
            );
            set_loaded.set(true);
        });
    };
    load();

    let clear = move || {
        // Optimistic: the badge is yours and the write is local. Waiting for it
        // to come back before the list greys out makes a local operation feel
        // like a network one.
        set_items.update(|v| v.iter_mut().for_each(|i| i.read = true));
        spawn_local(async move {
            if let Err(e) = notifications::mark_all_read().await {
                leptos::logging::warn!("mark_all_read failed: {e:?}");
            }
        });
    };

    view! {
        <section class="plane" style="flex:1">
            <header class="bar">
                <h1>"Activity"</h1>
                <div class="spring"></div>
                <button class="btn ghost" on:click=move |_| clear()>
                    "Mark all read"
                </button>
            </header>
            <div class="body">
                <Show
                    when=move || loaded.get() && items.get().is_empty()
                    fallback=|| ().into_view()
                >
                    <p class="empty">
                        "Nothing yet. Follows, reactions and comments land here."
                    </p>
                </Show>
                <For each=move || items.get() key=|i| (i.id.clone(), i.read) let:i>
                    <div class="card note-row" class:unread=move || !i.read>
                        <span class="avatar">{initial(&i.who)}</span>
                        <span class="who">
                            <b>{i.who.clone()}</b>
                            <i>{phrase(&i.kind)}</i>
                        </span>
                        <span class="sub">{crate::ago(i.ts)}</span>
                    </div>
                </For>
            </div>
        </section>
    }
}

/// The event type as a sentence.
///
/// The engine's event names are wire identifiers, not English. Showing
/// "follow.request" to a person is showing them the protocol.
fn phrase(kind: &str) -> &'static str {
    match kind {
        "follow.request" => "asked to follow you",
        "follow.accept" => "accepted your follow",
        "reaction" | "post.reaction" => "reacted to your post",
        "comment" | "post.comment" => "commented on your post",
        "dm" | "message" => "sent you a message",
        _ => "did something",
    }
}

fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}
