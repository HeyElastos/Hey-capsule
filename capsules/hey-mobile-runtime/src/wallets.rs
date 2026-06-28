//! External-wallet registry — a DEK-sealed `hey-social/wallets.json` that lets the
//! seed wallet and one or more Ledger wallets coexist. Holds NO key material: only
//! address + compressed pubkey + BIP44 path + label, so even if the file leaks
//! nothing spendable is exposed. The seed wallet stays the implicit default
//! ("This device") and is NOT stored here — it derives from the runtime identity.
//! See docs/HEY_LEDGER_SUPPORT.md §4.

use std::sync::OnceLock;

use hey_core::runtime::{shared_read_json, shared_write_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const FILE: &str = "hey-social/wallets.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub id: String,
    pub label: String,
    pub kind: String,       // "ledger-ela" (future: "ledger-evm")
    pub path: String,       // BIP44 path the device key lives at
    pub address: String,    // ELA mainchain E… address
    pub pubkey_hex: String, // 33-byte compressed P-256 pubkey (NOT secret)
    pub device_name: String,
    pub added_at: i64,
}

/// RMW lock for wallets.json. A separate file from social's storage, so its own lock
/// (no cross-file read-modify-write to coordinate).
fn lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn load() -> Vec<Wallet> {
    match shared_read_json(FILE).await {
        Ok(Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn store(list: &[Wallet]) -> Result<(), String> {
    shared_write_json(FILE, &json!(list)).await.map_err(|e| format!("save wallets: {e}"))
}

/// All registered external wallets (JSON array).
pub async fn list() -> Value {
    json!(load().await)
}

/// Add — or update the label/device of — a Ledger ELA wallet. Dedup by address
/// (re-adding the same device just refreshes its entry). Returns the stored wallet.
pub async fn add_ledger(
    kind: &str,
    label: &str,
    path: &str,
    address: &str,
    pubkey_hex: &str,
    device_name: &str,
) -> Result<Value, String> {
    if address.trim().is_empty() || pubkey_hex.trim().is_empty() {
        return Err("ledger wallet missing address/pubkey".into());
    }
    let kind = if kind.trim().is_empty() { "ledger-ela" } else { kind.trim() };
    // Must be ASCII hex: guarantees the id-prefix byte-slice below lands on a char
    // boundary (a non-ASCII string would panic → abort across the JNI cdylib) and
    // keeps ids stable. The real add-flow always passes lowercase hex.
    if !pubkey_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("ledger pubkey must be hex".into());
    }
    let _g = lock().lock().await;
    let mut list = load().await;
    let w = Wallet {
        id: format!("{kind}-{}", &pubkey_hex[..pubkey_hex.len().min(12)]),
        label: if label.trim().is_empty() { "Ledger".into() } else { label.trim().to_string() },
        kind: kind.to_string(),
        path: path.to_string(),
        address: address.to_string(),
        pubkey_hex: pubkey_hex.to_string(),
        device_name: device_name.to_string(),
        added_at: hey_core::plat::now_ms() as i64,
    };
    // Re-add of the same address = update in place; otherwise append.
    if let Some(slot) = list.iter_mut().find(|e| e.address == w.address) {
        *slot = w.clone();
    } else {
        list.push(w.clone());
    }
    store(&list).await?;
    Ok(json!(w))
}

/// Remove a wallet by id → `{removed: bool}`.
pub async fn remove(id: &str) -> Result<Value, String> {
    let _g = lock().lock().await;
    let mut list = load().await;
    let before = list.len();
    list.retain(|e| e.id != id);
    let removed = list.len() != before;
    if removed {
        store(&list).await?;
    }
    Ok(json!({ "removed": removed }))
}
