// Shared app-level modal triggers. The App component provides a single
// AppModals context; TopHeader's bell/search/add-friend buttons toggle
// the signals; the App-level modal components read them.
//
// Reactivity flows one way: button → signal → modal.

use leptos::prelude::*;

#[derive(Copy, Clone)]
pub struct AppModals {
    pub notifications_open: RwSignal<bool>,
    pub search_open: RwSignal<bool>,
    pub add_friend_open: RwSignal<bool>,
}

impl Default for AppModals {
    fn default() -> Self {
        Self {
            notifications_open: RwSignal::new(false),
            search_open: RwSignal::new(false),
            add_friend_open: RwSignal::new(false),
        }
    }
}
