// Direct-message API.
//
// Wire format (sent via peer.publish, received via peer.recv → routed by
// peer_receiver):
//
//   { type: "dm.message", payload: { text, ts } }
//
// Storage:
//   Hey/dm/contacts.json — [ { did, name, lastTs, lastPreview, unread } ]
//   Hey/dm/by-did/<did>.json — [ { id, text, ts, mine } ]
//
// E2E ENCRYPTION IS NOT YET PORTED. Messages are Ed25519-signed but
// transmitted as plaintext. The Hey Messenger capsule uses ML-KEM-768 +
// X25519 hybrid; porting those to Rust/WASM is a multi-day task that
// stays deferred. The Chat UI surfaces a "Signed, E2E coming" badge so
// users know the threat model.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::profile::ensure_profile;
use crate::events::create_signed_event;
use crate::runtime::{peer, storage, RuntimeError};
use crate::session;

const CONTACTS_FILE: &str = "dm/contacts.json";

fn conv_path(did: &str) -> String {
    let safe = did.replace(['/', ':'], "_");
    format!("dm/by-did/{safe}.json")
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmContact {
    pub did: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "lastTs")]
    pub last_ts: i64,
    #[serde(default, rename = "lastPreview")]
    pub last_preview: String,
    #[serde(default)]
    pub unread: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmMessage {
    pub id: String,
    pub text: String,
    pub ts: i64,
    pub mine: bool,
}

pub async fn list_contacts() -> Vec<DmContact> {
    storage::read_json(CONTACTS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<Vec<DmContact>>(v).ok())
        .unwrap_or_default()
}

async fn write_contacts(list: &[DmContact]) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(list)
        .map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(CONTACTS_FILE, &v).await
}

pub async fn read_conversation(did: &str) -> Vec<DmMessage> {
    storage::read_json(&conv_path(did))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

async fn write_conversation(did: &str, msgs: &[DmMessage]) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(msgs).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(&conv_path(did), &v).await
}

async fn upsert_contact(did: &str, last_preview: &str, ts: i64, inc_unread: u32) {
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        c.last_ts = ts;
        c.last_preview = last_preview.chars().take(140).collect();
        c.unread = c.unread.saturating_add(inc_unread);
    } else {
        list.push(DmContact {
            did: did.into(),
            name: String::new(),
            last_ts: ts,
            last_preview: last_preview.chars().take(140).collect(),
            unread: inc_unread,
        });
    }
    list.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    let _ = write_contacts(&list).await;
}

pub async fn mark_read(did: &str) {
    let mut list = list_contacts().await;
    if let Some(c) = list.iter_mut().find(|c| c.did == did) {
        c.unread = 0;
        let _ = write_contacts(&list).await;
    }
}

pub async fn total_unread() -> u32 {
    list_contacts().await.iter().map(|c| c.unread).sum()
}

// Send a message: local-write first, then publish to recipient's topic.
// Mirrors how the React reference works (own messages render instantly,
// remote sees them when peer.recv catches up on their end).
pub async fn send_message(peer_did: &str, text: &str) -> Result<DmMessage, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty message".into());
    }
    let me = ensure_profile().await.map_err(|e| e.to_string())?;
    if peer_did == me.did_key {
        return Err("cannot DM yourself".into());
    }
    let s = session::current().ok_or_else(|| "not signed in".to_string())?;

    let msg = DmMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: trimmed.chars().take(4096).collect(),
        ts: now_ms(),
        mine: true,
    };

    // 1. Local write.
    let mut conv = read_conversation(peer_did).await;
    conv.push(msg.clone());
    write_conversation(peer_did, &conv)
        .await
        .map_err(|e| e.to_string())?;
    upsert_contact(peer_did, &msg.text, msg.ts, 0).await;

    // 2. Sign + publish on the recipient's DM topic.
    let evt = create_signed_event(
        "dm.message",
        json!({ "text": msg.text, "ts": msg.ts }),
        &s.auth_key_hex,
    )?;
    let wire = crate::events::to_wire_string(&evt);
    let _ = peer::join_topic(&format!("hey-v0/dm/{peer_did}")).await;
    let _ = peer::publish(peer::PublishArgs {
        topic: &format!("hey-v0/dm/{peer_did}"),
        message: &wire,
        sender_id: &evt.sender_did,
        ts: evt.ts,
        signature: &evt.signature,
    })
    .await;
    Ok(msg)
}

// Receive a message (called by peer_receiver). Caller has already
// verified the Ed25519 signature against sender_did. Appends to the
// conversation + bumps unread.
pub async fn receive_message(sender_did: &str, payload: &Value) -> Result<(), String> {
    let text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "dm.message missing text".to_string())?;
    let ts = payload
        .get("ts")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(now_ms);
    let msg = DmMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.chars().take(4096).collect(),
        ts,
        mine: false,
    };
    let mut conv = read_conversation(sender_did).await;
    conv.push(msg.clone());
    write_conversation(sender_did, &conv)
        .await
        .map_err(|e| e.to_string())?;
    upsert_contact(sender_did, &msg.text, msg.ts, 1).await;
    Ok(())
}
