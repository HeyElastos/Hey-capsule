// Minimal session state — Rust port of capsules/hey-social/client/src/lib/session.js
// (slimmed to what's needed for sign-in gating).
//
// Stores { auth_key_hex, did_key, name } in localStorage so a page reload
// preserves the signed-in identity. Source of truth for "am I signed in?"
// is whether `current()` returns Some.

use gloo_storage::{LocalStorage, Storage as _};
use serde::{Deserialize, Serialize};

const SESSION_KEY: &str = "hey-social-rust-session";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub auth_key_hex: String,
    pub did_key: String,
    pub name: String,
}

pub fn current() -> Option<Session> {
    LocalStorage::get::<Session>(SESSION_KEY).ok()
}

pub fn set(session: &Session) {
    let _ = LocalStorage::set(SESSION_KEY, session);
}

pub fn clear() {
    let _ = LocalStorage::delete(SESSION_KEY);
}
