// Shared app-level modal triggers. The App component provides a single
// AppModals context; TopHeader's bell/search/add-friend buttons toggle
// the signals; the App-level modal components read them.
//
// Reactivity flows one way: button → signal → modal.

use gloo_storage::{LocalStorage, Storage as _};
use leptos::prelude::*;

const DOCK_OPEN_KEY: &str = "hey-dock-open";

#[derive(Copy, Clone)]
pub struct AppModals {
    pub notifications_open: RwSignal<bool>,
    pub search_open: RwSignal<bool>,
    pub add_friend_open: RwSignal<bool>,
    /// "Contacts" panel — quick list of the people you chat with, openable
    /// from the dock so you can jump into a conversation from any page.
    pub contacts_open: RwSignal<bool>,
    /// "Following" panel — the social graph (people you follow), distinct from
    /// the DM Contacts panel. Openable from the dock.
    pub following_open: RwSignal<bool>,
    pub new_group_open: RwSignal<bool>,
    /// "Link phone" QR modal — shows a code the Hey phone app scans to sign in.
    pub link_phone_open: RwSignal<bool>,
    /// Whether the FloatingDock is expanded. Persisted in localStorage so
    /// user preference survives a reload. Default: open.
    pub dock_open: RwSignal<bool>,
}

impl Default for AppModals {
    fn default() -> Self {
        let dock_open = LocalStorage::get::<bool>(DOCK_OPEN_KEY).unwrap_or(true);
        let dock_open = RwSignal::new(dock_open);
        // Persist on every change.
        Effect::new(move |_| {
            let _ = LocalStorage::set(DOCK_OPEN_KEY, dock_open.get());
        });
        Self {
            notifications_open: RwSignal::new(false),
            search_open: RwSignal::new(false),
            add_friend_open: RwSignal::new(false),
            contacts_open: RwSignal::new(false),
            following_open: RwSignal::new(false),
            new_group_open: RwSignal::new(false),
            link_phone_open: RwSignal::new(false),
            dock_open,
        }
    }
}
