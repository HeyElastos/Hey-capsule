//! Small, local, non-secret UI preferences.
//!
//! Device names, workspace column width, pins. Nothing secret lives here —
//! identity and messages stay in runtime storage, sealed by ElastOS. A name
//! and not an index: unplug a camera and every index after it shifts.

use hey_core::runtime::storage;
use serde_json::Value;
use wasm_bindgen::JsCast;

const LS_KEY: &str = "hyper-desktop-ui-prefs";

fn load_local() -> Value {
    let Some(win) = web_sys::window() else {
        return serde_json::json!({});
    };
    let Ok(Some(storage)) = win.local_storage() else {
        return serde_json::json!({});
    };
    storage
        .get_item(LS_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

fn store_local(key: &str, v: Value) {
    let mut root = load_local();
    if let Some(o) = root.as_object_mut() {
        o.insert(key.to_string(), v);
    }
    if let Some(win) = web_sys::window() {
        if let Ok(Some(storage)) = win.local_storage() {
            if let Ok(s) = serde_json::to_string(&root) {
                let _ = storage.set_item(LS_KEY, &s);
            }
        }
    }
    // Mirror to runtime storage so a wipe of localStorage is not a wipe of
    // the preference — same at-rest home as everything else this capsule owns.
    let snapshot = root.clone();
    leptos::task::spawn_local(async move {
        let _ = storage::write_json("ui-prefs.json", &snapshot).await;
    });
}

pub fn call_camera() -> String {
    load_local()
        .get("call_camera")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn set_call_camera(name: &str) {
    store_local("call_camera", serde_json::json!(name));
}

pub fn call_mic() -> String {
    load_local()
        .get("call_mic")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn set_call_mic(name: &str) {
    store_local("call_mic", serde_json::json!(name));
}

pub fn call_speaker() -> String {
    load_local()
        .get("call_speaker")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn set_call_speaker(name: &str) {
    store_local("call_speaker", serde_json::json!(name));
}

pub fn ws_files_w() -> f32 {
    load_local()
        .get("ws_files_w")
        .and_then(Value::as_f64)
        .map(|n| n as f32)
        .filter(|n| n.is_finite() && *n > 0.0)
        .unwrap_or(212.0)
}

pub fn set_ws_files_w(w: f32) {
    if w.is_finite() && w > 0.0 {
        store_local("ws_files_w", serde_json::json!(w));
    }
}

pub fn ws_files_open() -> bool {
    load_local()
        .get("ws_files_open")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn set_ws_files_open(open: bool) {
    store_local("ws_files_open", serde_json::json!(open));
}

/// Light theme. `None` means unset: follow the OS (`prefers-color-scheme`).
pub fn light() -> Option<bool> {
    load_local().get("light").and_then(Value::as_bool)
}

pub fn set_light(on: bool) {
    store_local("light", serde_json::json!(on));
    apply_skin();
}

/// Accent slug (`gold`, `sky`, …). Empty means gold, the brand default.
pub fn accent() -> String {
    load_local()
        .get("accent")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn set_accent(key: &str) {
    let k = match key {
        "champagne" | "sky" | "mono" | "violet" | "gold" => key,
        _ => "gold",
    };
    store_local("accent", serde_json::json!(k));
    apply_skin();
}

/// Chat detail rail. Default on. Hiding it is the choice that must survive.
pub fn rail_pinned() -> bool {
    load_local()
        .get("rail_pinned")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

pub fn set_rail_pinned(on: bool) {
    store_local("rail_pinned", serde_json::json!(on));
}

/// Last message already seen in each 1:1, keyed by peer DID.
/// The engine only stores an unread count. The New bar needs the message id.
pub fn last_read_id(did: &str) -> Option<String> {
    load_local()
        .get("chat_last_read")
        .and_then(Value::as_object)
        .and_then(|o| o.get(did))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

pub fn set_last_read_id(did: &str, id: &str) {
    if did.is_empty() || id.is_empty() {
        return;
    }
    let mut root = load_local();
    let slot = root
        .as_object_mut()
        .map(|o| o.entry("chat_last_read").or_insert_with(|| serde_json::json!({})));
    if let Some(Value::Object(m)) = slot {
        m.insert(did.to_string(), serde_json::json!(id));
    }
    if let Some(win) = web_sys::window() {
        if let Ok(Some(storage)) = win.local_storage() {
            if let Ok(s) = serde_json::to_string(&root) {
                let _ = storage.set_item(LS_KEY, &s);
            }
        }
    }
    let snapshot = root.clone();
    leptos::task::spawn_local(async move {
        let _ = storage::write_json("ui-prefs.json", &snapshot).await;
    });
}

/// Write `data-theme` and `data-accent` on `<html>` so CSS matches the
/// desktop's persisted appearance. Call once at boot and after every set.
pub fn apply_skin() {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Some(doc) = win.document() else {
        return;
    };
    let Some(el) = doc.document_element() else {
        return;
    };
    match light() {
        Some(true) => {
            let _ = el.set_attribute("data-theme", "light");
        }
        Some(false) => {
            let _ = el.set_attribute("data-theme", "dark");
        }
        None => {
            let _ = el.remove_attribute("data-theme");
        }
    }
    let acc = accent();
    if acc.is_empty() || acc == "gold" {
        let _ = el.remove_attribute("data-accent");
    } else {
        let _ = el.set_attribute("data-accent", &acc);
    }
}

/// Pull a FileList off an `<input type=file>`. Empty if the event is not one.
pub fn files_from_input(ev: web_sys::Event) -> Vec<web_sys::File> {
    let Some(t) = ev.target() else { return Vec::new() };
    let Ok(input) = t.dyn_into::<web_sys::HtmlInputElement>() else {
        return Vec::new();
    };
    let Some(list) = input.files() else { return Vec::new() };
    let mut out = Vec::new();
    for i in 0..list.length() {
        if let Some(f) = list.item(i) {
            out.push(f);
        }
    }
    input.set_value("");
    out
}
