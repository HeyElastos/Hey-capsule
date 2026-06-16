//! Hey Verse <-> Hey core bridge (GDExtension, via godot-rust/gdext).
//!
//! The game talks to one Godot class, `HeyVerseBridge`. net.gd instantiates
//! it if registered and falls back to a built-in sim peer otherwise, so the
//! game runs with or without this library.
//!
//! This default build is a STUB: it compiles standalone (no Hey deps) and
//! returns a deterministic dev identity. The `with-runtime` feature is where
//! the real wiring lands, reusing what Hey already has:
//!
//!   identity   social::whoami_did()            (capsules/hey-mobile-runtime)
//!   invites    chat_gen_invite / chat_accept_invite
//!   chat       chat_send / poll_once           (sealed PQ DMs — bubbles)
//!   transport  carrier.endpoint()              (raw iroh Endpoint)
//!              + peer_ticket(did)              (dial address per contact)
//!
//! Movement lane (to implement under `with-runtime`):
//!   - open a bidirectional "verse" route on the carrier endpoint
//!   - send unreliable datagrams {"t":"mv","x":f32,"z":f32,"yw":f32,"m":bool}
//!     at <= 15 Hz; drop-tolerant by design, no head-of-line blocking
//!   - presence join/leave over gossip carrying {name, color-from-DID}
//!   - poll() drains a tokio mpsc fed by the receive task into a Dictionary
//!     shaped exactly like net.gd's sim: did -> {pos,yaw,moving,name,color}
//!
//! Build: ./build.sh   (copies the .so + .gdextension into ../hey-verse)
//! Android: cargo-ndk -t arm64-v8a build --release (the real target; desktop
//! flatpak-Godot may refuse a host-glibc .so — test desktop with native Godot).

use godot::prelude::*;

struct HeyVerseExtension;

#[gdextension]
unsafe impl ExtensionLibrary for HeyVerseExtension {}

/// Same pastel palette as net.gd / Hey's chat avatars — one identity, one color.
const PALETTE: [(f32, f32, f32); 8] = [
    (0.50, 0.71, 1.00),
    (0.94, 0.78, 0.31),
    (1.00, 0.54, 0.81),
    (0.50, 0.89, 0.75),
    (0.76, 0.61, 1.00),
    (1.00, 0.64, 0.50),
    (0.54, 0.84, 1.00),
    (0.72, 0.91, 0.53),
];

const STUB_DID: &str = "did:key:z6MkVerseDevStub";

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn did_color(did: &str) -> Color {
    let (r, g, b) = PALETTE[(fnv1a(did) % PALETTE.len() as u64) as usize];
    Color::from_rgb(r, g, b)
}

/// Local inventory (Phase 0). A JSON file in the Hey data dir, co-located with the
/// runtime identity so it's the user's. Phase 1 swaps this for on-chain ownership
/// (ESC licenses) + Elacity .ddrm key delivery; the item shape already matches
/// items.gd's marketplace record (id/kind/name/builtin/…/token_id/ddrm_cid).
mod inventory {
    use serde_json::{json, Value};
    use std::path::PathBuf;

    fn inv_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("hey-social-native")
            .join("verse-inventory.json")
    }

    fn owned_ids() -> Vec<String> {
        std::fs::read_to_string(inv_path())
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
    }

    fn save_ids(ids: &[String]) {
        let p = inv_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, serde_json::to_string(ids).unwrap_or_default());
    }

    /// The shop catalog. Phase-0 items are builtins (built in home.gd); a real
    /// .ddrm item is the same record with token_id/ddrm_cid/glb_path filled in.
    pub fn catalog() -> Vec<Value> {
        vec![
            json!({"id":"cozy_kitchen","kind":"furniture","name":"Cozy Kitchen","builtin":"kitchen","price_ela":"0.50","pos":[1.7,0.0,-1.3],"rot_y":0.0}),
            json!({"id":"reading_nook","kind":"furniture","name":"Reading Nook","builtin":"cushion","price_ela":"0.20","pos":[-1.4,0.0,1.0],"rot_y":0.4}),
            json!({"id":"crate_stack","kind":"furniture","name":"Crate Stack","builtin":"crate","price_ela":"0.10","pos":[2.1,0.0,0.9],"rot_y":0.0}),
        ]
    }

    pub fn shop_json() -> String {
        Value::Array(catalog()).to_string()
    }

    pub fn owned_json() -> String {
        let owned = owned_ids();
        let items: Vec<Value> = catalog()
            .into_iter()
            .filter(|c| c.get("id").and_then(Value::as_str).map(|id| owned.iter().any(|o| o == id)).unwrap_or(false))
            .collect();
        Value::Array(items).to_string()
    }

    pub fn is_owned(id: &str) -> bool {
        owned_ids().iter().any(|o| o == id)
    }

    /// Phase 0: grant locally (free). Phase 1: pay via the wallet + verify an
    /// on-chain license before granting.
    pub fn buy(id: &str) -> bool {
        if !catalog().iter().any(|c| c.get("id").and_then(Value::as_str) == Some(id)) {
            return false;
        }
        let mut owned = owned_ids();
        if !owned.iter().any(|o| o == id) {
            owned.push(id.to_string());
            save_ids(&owned);
        }
        true
    }
}

#[derive(GodotClass)]
#[class(init, base=Node)]
pub struct HeyVerseBridge {
    base: Base<Node>,
}

#[godot_api]
impl HeyVerseBridge {
    #[func]
    fn local_did(&self) -> GString {
        // with-runtime: block_on(social::whoami_did())
        STUB_DID.into()
    }

    #[func]
    fn local_name(&self) -> GString {
        // with-runtime: nickname from social::get_profile(self)
        "you".into()
    }

    #[func]
    fn local_color(&self) -> Color {
        did_color(STUB_DID)
    }

    /// Your Hey contacts (did -> display name). The ONLY invitable people —
    /// contacts exist solely via hey friend-invite links.
    /// with-runtime: social::chat_contacts() + following()
    #[func]
    fn contacts(&self) -> VarDictionary {
        VarDictionary::new()
    }

    /// Offer a LIVE visit to one contact: send {t:"verse-inv", ticket} over
    /// the sealed DM lane; their accept dials our carrier endpoint. The grant
    /// lives exactly as long as the connection — disconnect voids it, nothing
    /// persists, re-invite to rejoin (matches the sim's session model).
    /// with-runtime: social::call_send(did, verse_inv_json) + peer_ticket()
    #[func]
    fn invite(&mut self, _did: GString) {}

    /// Movement lane out. Called at <=15 Hz while walking, 1 Hz idle.
    #[func]
    fn send_move(&mut self, _x: f32, _z: f32, _yaw: f32, _moving: bool) {
        // with-runtime: endpoint datagram {"t":"mv",...} to each room peer
    }

    /// Chat out — rides Hey's existing sealed-DM lane, not the datagrams.
    #[func]
    fn send_chat(&mut self, _text: GString) {
        // with-runtime: social::chat_send(peer_did, text) per room member
    }

    /// Drain inbound state. Shape must match net.gd's sim exactly:
    /// { did: {pos: Vector3, yaw: f32, moving: bool, name: GString, color: Color} }
    #[func]
    fn poll(&mut self) -> VarDictionary {
        VarDictionary::new()
    }

    // ── marketplace / inventory ───────────────────────────────────────────────
    /// Catalog of buyable items (JSON array string — `JSON.parse_string` it).
    #[func]
    fn shop_items(&self) -> GString {
        inventory::shop_json().as_str().into()
    }

    /// The items the user owns (JSON array string), shaped like items.gd records.
    #[func]
    fn owned_items(&self) -> GString {
        inventory::owned_json().as_str().into()
    }

    #[func]
    fn is_owned(&self, id: GString) -> bool {
        inventory::is_owned(&id.to_string())
    }

    /// Buy an item. Phase 0 grants locally; Phase 1 pays via the wallet + verifies
    /// an on-chain (ESC) license before granting.
    #[func]
    fn buy_item(&mut self, id: GString) -> bool {
        inventory::buy(&id.to_string())
    }
}
