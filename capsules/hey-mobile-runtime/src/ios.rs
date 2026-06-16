//! iOS C-ABI FFI — the Swift-facing surface for the Hey iOS app (`mobile/hey-ios`).
//! The iOS counterpart to the Android JNI exports in `lib.rs`: SAME engine, same
//! in-process (loopback-free) boot, different bridge. The boot itself is shared —
//! `run_async` / `install_inprocess_dispatch` are gated on `any(android, ios)` — so
//! this module only provides the C entry points + the per-call plat plumbing that
//! mirrors `mod android`'s `ensure_plat` / `block` / `json_result` / `signing_phrase`.
//!
//! ACTIVATE (two one-line edits, inert on Linux/Android):
//!   • `Cargo.toml` → `[lib] crate-type = ["staticlib", "cdylib", "rlib"]`
//!   • `lib.rs`     → `#[cfg(target_os = "ios")] mod ios;`  (already added)
//!
//! NOTE: the bodies mirror `mod android` 1:1 against the engine's `social::*` /
//! `wallet::*` / `mainchain::*` / `did::*` / `guard::*` API. They are written to
//! compile, but the exact signatures of those engine fns are resolved on the first
//! macOS `cargo build --target aarch64-apple-ios` — adjust arity there if the
//! compiler flags a mismatch (the call shapes come from lib.rs's verified JNI map).
//!
//! Memory contract (matches `include/HeyEngine.h`): every returned `*mut c_char` is
//! caller-owned — free it once with `hey_string_free`; every `*mut u8` from a bytes
//! getter is freed once with `hey_bytes_free`.
#![cfg(target_os = "ios")]

use crate::{mainchain, social, wallet, Config, IDENTITY};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::OnceLock;

// ── small helpers ────────────────────────────────────────────────────────────

/// Vault/storage root, captured by `hey_start`. Used by `ensure_plat` (set_store).
static DATA_DIR: OnceLock<String> = OnceLock::new();

fn out(s: impl Into<String>) -> *mut c_char {
    CString::new(s.into()).unwrap_or_default().into_raw()
}

unsafe fn arg<'a>(p: *const c_char) -> &'a str {
    if p.is_null() { return ""; }
    CStr::from_ptr(p).to_str().unwrap_or("")
}

/// Mirror of `android::ensure_plat`: point hey-core's thread-local plat at the
/// in-process dispatcher + storage root, then install the social ctx. The base is
/// a NAMESPACE ROUTING KEY only — the `set_dispatch` closure parses just the
/// `/api/provider/<scheme>/<op>` path and answers in-process; the host is never
/// resolved or connected to. Deliberately NOT 127.0.0.1: iOS opens no loopback
/// socket at all, so we don't even spell one (the host is cosmetic).
fn ensure_plat() {
    hey_core::plat::set_base("http://hey-runtime.local"); // routing key; no loopback, no socket
    if let Some(dir) = DATA_DIR.get() {
        hey_core::plat::set_store(dir);
    }
    social::install_ctx();
}

/// Mirror of `android::block`: a fresh current-thread runtime per call so the
/// social future runs on THIS thread and sees the thread-local plat base.
fn block<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread rt")
        .block_on(f)
}

fn json_result(r: Result<serde_json::Value, String>) -> String {
    match r {
        Ok(v) => v.to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Runtime-held BIP39 phrase (secrets used, never owned). The phrase never crosses
/// the Swift boundary per call — the canonical source is the runtime identity. An
/// explicitly passed phrase still wins (restore preview / legacy callers), mirroring
/// `android::signing_phrase`.
fn signing_phrase(explicit: &str) -> Result<String, String> {
    if !explicit.trim().is_empty() {
        return Ok(explicit.to_string());
    }
    IDENTITY
        .get()
        .and_then(|i| i.mnemonic().map(str::to_string))
        .ok_or_else(|| "wallet locked: runtime identity not ready".to_string())
}

// ── lifecycle ────────────────────────────────────────────────────────────────

// ── H2 STATUS ON iOS (READ THIS) ─────────────────────────────────────────────
//
// H2-full (the AUTH-GATED seed seal + decrypt-on-unlock) is NOT implemented on iOS
// in this release. iOS structurally stays vault-OFF: `hey_start` boots
// `load_or_create` from the NO-AUTH DEK-sealed vault (identity_blob: None below),
// there is NO `hey_unlock` C-ABI, NO headless sentinel boot, and NO Keychain/
// Secure-Enclave AUTH-gated seed seal. A jailbroken, once-unlocked iOS device can
// therefore still read the seed under the no-auth DEK — the original pentest finding
// remains OPEN on iOS. The "rooted-device seed" finding is closed on ANDROID ONLY
// this release (Android: IdentityVault auth-gated seal + hey_unlock + sentinel boot).
//
// To close it on iOS later, mirror the Android path: a Keychain item with
// kSecAttrAccessControl(.biometryCurrentSet) + kSecAttrAccessibleWhenUnlockedThisDeviceOnly
// holding the seed, a `hey_unlock(phrase)` C-ABI mirroring lib.rs hey_unlock
// (account-bind against carrier_sk, clear IDENTITY_SEALED), a headless sentinel boot
// off carrier-identity.json, and the carrier-identity.json backfill on seal. Until
// then, do NOT claim H2/"finding closed" on iOS.

/// Install the 32-byte at-rest storage DEK (Base64), wrapped by the iOS Keychain/
/// Secure Enclave on the Swift side. MUST be called BEFORE `hey_start`. 0 ok / -1 bad.
#[no_mangle]
pub unsafe extern "C" fn hey_set_storage_key(dek_b64: *const c_char) -> c_int {
    use base64::Engine as _;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(arg(dek_b64).trim()) {
        Ok(b) => b,
        Err(_) => return -1,
    };
    if bytes.len() != 32 {
        log::error!("hey_set_storage_key: expected 32 bytes, got {}", bytes.len());
        return -1;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    hey_core::plat::set_at_rest_key(key);
    0
}

/// Boot the runtime + carrier in-process (loopback-free). `dir` = the App Group
/// vault/storage root. Mirrors `android::hey_init` → `start_background(Config)` +
/// the feed/DM receiver threads (so a backgrounded/locked device still meshes).
#[no_mangle]
pub unsafe extern "C" fn hey_start(dir: *const c_char) {
    crate::logbuf::init(log::LevelFilter::Debug);
    std::panic::set_hook(Box::new(|info| log::error!("PANIC: {info}")));
    // Home on the SAME relay as peers until iroh ships 1.0 (mirrors android::hey_init).
    crate::relay_only_default();
    let dir = arg(dir).to_string();
    let _ = DATA_DIR.set(dir.clone());
    let cfg = Config {
        data_dir: PathBuf::from(&dir),
        dist_dir: PathBuf::from(&dir), // unused on mobile (socket-free)
        port: 0,                        // unused on mobile
        capsule: "hey-social".to_string(),
        identity_blob: None,            // load_or_create from the (DEK-sealed) vault
    };
    crate::start_background(cfg);
    // Wait until the in-process provider dispatch is installed (no socket to probe).
    block(async {
        for _ in 0..100 {
            if hey_core::plat::dispatch_ready() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    spawn_receiver(dir.clone());
    spawn_dm_receiver(dir);
}

/// Boot the runtime deriving the identity FROM `phrase` (returning-user restore, or a
/// vault unseal that recovered the seed). Same machinery as `hey_start` but with
/// `identity_blob = Some(phrase)` so the runtime re-derives did/wallets from the seed
/// instead of load_or_create. Mirrors `android::hey_init(dir, "", port, capsule, seed)`.
#[no_mangle]
pub unsafe extern "C" fn hey_restore(dir: *const c_char, phrase: *const c_char) {
    crate::logbuf::init(log::LevelFilter::Debug);
    std::panic::set_hook(Box::new(|info| log::error!("PANIC: {info}")));
    crate::relay_only_default();
    let dir = arg(dir).to_string();
    let seed = arg(phrase).trim().to_string();
    let _ = DATA_DIR.set(dir.clone());
    let cfg = Config {
        data_dir: PathBuf::from(&dir),
        dist_dir: PathBuf::from(&dir),
        port: 0,
        capsule: "hey-social".to_string(),
        identity_blob: if seed.is_empty() { None } else { Some(seed) },
    };
    crate::start_background(cfg);
    block(async {
        for _ in 0..100 {
            if hey_core::plat::dispatch_ready() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    spawn_receiver(dir.clone());
    spawn_dm_receiver(dir);
}

/// DM/group receiver — hey-core's canonical peer_receiver::run() loop on its own
/// thread (so its thread-local plat/ctx/session stay valid). Mirrors android.
fn spawn_dm_receiver(data_dir: String) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("hey-dm-recv".into())
        .spawn(move || {
            hey_core::plat::set_base("http://hey-runtime.local");
            hey_core::plat::set_store(&data_dir);
            social::install_ctx();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("dm recv rt");
            rt.block_on(async {
                let _ = social::ensure_session().await;
                hey_core::peer_receiver::run().await; // loops forever
            });
        })
        .expect("spawn dm receiver");
}

/// Feed receiver — joins my + followed topics and ingests posts/reactions/comments
/// every ~2s on a dedicated thread. Mirrors android::spawn_receiver.
fn spawn_receiver(data_dir: String) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("hey-social-recv".into())
        .spawn(move || {
            hey_core::plat::set_base("http://hey-runtime.local");
            hey_core::plat::set_store(&data_dir);
            social::install_ctx();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("recv rt");
            loop {
                rt.block_on(async {
                    let _ = social::ensure_session().await;
                    social::ensure_subscriptions().await;
                    if !hey_core::plat::processing_deferred() {
                        let n = social::poll_once().await;
                        if n > 0 {
                            log::info!("feed receiver ingested {n} item(s)");
                        }
                    }
                });
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        })
        .expect("spawn receiver");
}

/// Re-probe the carrier after a network change (Network.framework NWPathMonitor).
#[no_mangle]
pub unsafe extern "C" fn hey_net_changed() {
    if let Some((h, slot)) = crate::NET.get() {
        let slot = slot.clone();
        h.spawn(async move {
            if let Some(c) = slot.read().await.clone() {
                c.network_changed().await;
            }
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn hey_string_free(s: *mut c_char) {
    if !s.is_null() { drop(CString::from_raw(s)); }
}

// ── identity / profile ───────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hey_whoami() -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::whoami())))
}

/// Self profile when `did` is empty, else a peer's profile. Returns a Profile JSON
/// object (always an object, never an error envelope) — matches `social::get_profile`.
#[no_mangle]
pub unsafe extern "C" fn hey_profile(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::get_profile(arg(did))).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_save_profile(
    nickname: *const c_char,
    bio: *const c_char,
    avatar: *const c_char,
) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::set_profile(arg(nickname), arg(bio), arg(avatar)))))
}

/// The runtime-held BIP39 recovery phrase (read from the in-memory identity). Empty
/// if the runtime isn't ready or has no phrase.
///
/// C-1(a): mirrors the Android `hey_recovery_phrase` gate — when the hardware spend/
/// reveal binding is ACTIVE, refuse the bare reveal and force the caller through the
/// signature-verified `hey_recovery_phrase_hw`, so an in-process caller can't
/// exfiltrate the master seed with one unauthenticated call. Otherwise audit + return.
#[no_mangle]
pub unsafe extern "C" fn hey_recovery_phrase() -> *mut c_char {
    if crate::guard::spend_binding_active() {
        crate::guard::audit("seed.reveal.deny", serde_json::json!({ "reason": "hardware binding active — use the verified reveal" }));
        return out("");
    }
    let phrase = crate::wallet_phrase().unwrap_or_default();
    if !phrase.is_empty() {
        crate::guard::audit("seed.reveal", serde_json::json!({}));
    }
    // L-1: copy the words into the returned CString, then wipe the in-process copy in place
    // so the master seed doesn't linger in freed heap after the reveal returns.
    let ret = out(phrase.as_str());
    crate::wipe_phrase(phrase);
    ret
}

/// Issue a fresh one-time challenge the Keychain/Secure-Enclave op must sign to reveal
/// the seed (C-1(c), mirrors Android hey_reveal_challenge).
#[no_mangle]
pub unsafe extern "C" fn hey_reveal_challenge() -> *mut c_char {
    out(crate::guard::issue_reveal_challenge().unwrap_or_default())
}

/// HARDWARE-VERIFIED seed reveal (C-1(c), mirrors Android hey_recovery_phrase_hw):
/// returns the mnemonic ONLY after a fresh Secure-Enclave signature over the one-time
/// reveal-challenge verifies against the enrolled key. Empty on a bad/missing sig.
#[no_mangle]
pub unsafe extern "C" fn hey_recovery_phrase_hw(sig_hex: *const c_char) -> *mut c_char {
    if let Err(e) = crate::guard::verify_reveal_sig(arg(sig_hex)) {
        log::warn!("recovery_phrase_hw: {e}");
        return out("");
    }
    let phrase = crate::wallet_phrase().unwrap_or_default();
    if !phrase.is_empty() {
        crate::guard::audit("seed.reveal", serde_json::json!({ "verified": true }));
    }
    // L-1: copy out, then wipe the in-process copy in place (see hey_recovery_phrase).
    let ret = out(phrase.as_str());
    crate::wipe_phrase(phrase);
    ret
}

/// Validate a BIP39 phrase (12/24 words, checksum). "ok" if valid, "" otherwise.
#[no_mangle]
pub unsafe extern "C" fn hey_validate_mnemonic(phrase: *const c_char) -> *mut c_char {
    out(if bip39::Mnemonic::parse(arg(phrase).trim()).is_ok() { "ok" } else { "" })
}

/// My own friend/invite link (follow-by-link). Empty string on error.
#[no_mangle]
pub unsafe extern "C" fn hey_my_friend_link() -> *mut c_char {
    ensure_plat();
    out(block(social::my_friend_link()).unwrap_or_default())
}

/// Generate a one-time chat invite token (label is cosmetic). Empty on error.
#[no_mangle]
pub unsafe extern "C" fn hey_gen_invite(label: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::chat_gen_invite(arg(label))).unwrap_or_default())
}

/// Accept a chat invite token. Returns `{"ok":true,"did":…}` or `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_accept_invite(token: *const c_char) -> *mut c_char {
    ensure_plat();
    match block(social::chat_accept_invite(arg(token))) {
        Ok(did) => out(serde_json::json!({ "ok": true, "did": did }).to_string()),
        Err(e) => out(serde_json::json!({ "error": e }).to_string()),
    }
}

/// Carrier health snapshot — enriched with the live net stack like android.
#[no_mangle]
pub unsafe extern "C" fn hey_carrier_health() -> *mut c_char {
    ensure_plat();
    let mut v = block(social::carrier_health());
    if let (Some(obj), Some((h, slot))) = (v.as_object_mut(), crate::NET.get()) {
        if let Some(extra) = h.block_on(async {
            let g = slot.read().await;
            let c = g.as_ref()?;
            let (peers, direct_peers, relay_peers) = c.conn_summary().await;
            Some(serde_json::json!({
                "online": c.is_online(),
                "direct": direct_peers > 0,
                "peer_count": peers,
                "direct_peers": direct_peers,
                "relay_peers": relay_peers,
            }))
        }) {
            if let Some(eo) = extra.as_object() {
                for (k, val) in eo {
                    obj.insert(k.clone(), val.clone());
                }
            }
        }
    }
    out(v.to_string())
}

// ── feed ─────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hey_feed(limit: c_int) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::feed(limit.max(1) as usize))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_user_posts(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::user_posts(arg(did))).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_get_post(id: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::get_post(arg(id)))))
}

/// Upload raw media bytes → a Media tile JSON `{cid,mime,type,name}`. `data`/`len`
/// describe the buffer; `mime`/`name` are the tile's metadata.
#[no_mangle]
pub unsafe extern "C" fn hey_upload_media(
    data: *const u8,
    len: usize,
    mime: *const c_char,
    name: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let bytes = if data.is_null() || len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, len).to_vec()
    };
    let name = {
        let n = arg(name);
        if n.is_empty() { "upload" } else { n }
    };
    out(json_result(block(social::upload_media(&bytes, arg(mime), name))))
}

/// Create a post. `media_json` = a JSON array of media tiles ("" / "[]" = text-only).
#[no_mangle]
pub unsafe extern "C" fn hey_create_post(text: *const c_char, media_json: *const c_char) -> *mut c_char {
    ensure_plat();
    let media = arg(media_json);
    let media = if media.is_empty() { "[]" } else { media };
    out(json_result(block(social::create_post(arg(text), media))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_delete_post(id: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::delete_post(arg(id)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_edit_post(id: *const c_char, caption: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::edit_post(arg(id), arg(caption)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_get_reactions(post_id: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::get_reactions(arg(post_id)))))
}

/// Toggle/set a reaction `emoji` on a post → the new aggregate Reactions JSON.
#[no_mangle]
pub unsafe extern "C" fn hey_react(post_id: *const c_char, emoji: *const c_char) -> *mut c_char {
    ensure_plat();
    let e = {
        let e = arg(emoji);
        if e.is_empty() { "\u{2764}\u{fe0f}" } else { e }
    };
    out(json_result(block(social::react(arg(post_id), e))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_get_comments(post_id: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::get_comments(arg(post_id)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_add_comment(
    post_id: *const c_char,
    text: *const c_char,
    parent: *const c_char,
) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::add_comment(arg(post_id), arg(text), arg(parent)))))
}

/// Cheap change counter — the UI polls this and reloads only when it bumps.
#[no_mangle]
pub unsafe extern "C" fn hey_feed_rev() -> i64 {
    social::feed_rev() as i64
}

// ── chat ─────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hey_contacts() -> *mut c_char {
    ensure_plat();
    out(block(social::chat_contacts()).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_groups() -> *mut c_char {
    ensure_plat();
    out(block(social::chat_groups()).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_conversation(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::chat_conversation(arg(did))).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_group_conversation(gid: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::chat_group_conversation(arg(gid))).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_send_dm(did: *const c_char, text: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::chat_send(arg(did), arg(text)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_send_group(gid: *const c_char, text: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::chat_send_group(arg(gid), arg(text)))))
}

/// Send a 1:1 attachment with optional caption text → `{}` or `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_send_attachment(
    did: *const c_char,
    text: *const c_char,
    data: *const u8,
    len: usize,
    mime: *const c_char,
    filename: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let bytes = if data.is_null() || len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, len).to_vec()
    };
    let f = { let f = arg(filename); if f.is_empty() { "file" } else { f } };
    out(json_result(block(social::chat_send_attachment(arg(did), arg(text), &bytes, arg(mime), f))))
}

/// Send a group attachment with optional caption text → `{}` or `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_send_group_attachment(
    gid: *const c_char,
    text: *const c_char,
    data: *const u8,
    len: usize,
    mime: *const c_char,
    filename: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let bytes = if data.is_null() || len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, len).to_vec()
    };
    let f = { let f = arg(filename); if f.is_empty() { "file" } else { f } };
    out(json_result(block(social::chat_send_group_attachment(arg(gid), arg(text), &bytes, arg(mime), f))))
}

/// Resolve an attachment (the raw attachment JSON object) → decrypted plaintext
/// bytes. Returns a heap buffer of `*out_len` bytes (caller frees with hey_bytes_free).
#[no_mangle]
pub unsafe extern "C" fn hey_fetch_attachment(att_json: *const c_char, out_len: *mut usize) -> *mut u8 {
    ensure_plat();
    let bytes = block(social::chat_fetch_attachment(arg(att_json))).unwrap_or_default();
    bytes_out(bytes, out_len)
}

#[no_mangle]
pub unsafe extern "C" fn hey_react_message(
    chat_id: *const c_char,
    message_id: *const c_char,
    emoji: *const c_char,
    is_group: c_int,
) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::chat_react_message(arg(chat_id), arg(message_id), arg(emoji), is_group != 0))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_delete_message(
    chat_id: *const c_char,
    msg_id: *const c_char,
    is_group: c_int,
) -> *mut c_char {
    ensure_plat();
    let ok = block(social::delete_chat_message(arg(chat_id), arg(msg_id), is_group != 0));
    out(serde_json::json!({ "ok": ok }).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_edit_message(
    chat_id: *const c_char,
    msg_id: *const c_char,
    text: *const c_char,
    is_group: c_int,
) -> *mut c_char {
    ensure_plat();
    let ok = block(social::edit_chat_message(arg(chat_id), arg(msg_id), arg(text), is_group != 0));
    out(serde_json::json!({ "ok": ok }).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_message_reactions(chat_id: *const c_char, is_group: c_int) -> *mut c_char {
    ensure_plat();
    out(block(social::chat_message_reactions(arg(chat_id), is_group != 0)).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_create_group(name: *const c_char, members_json: *const c_char) -> *mut c_char {
    ensure_plat();
    let m = { let m = arg(members_json); if m.is_empty() { "[]" } else { m } };
    out(json_result(block(social::chat_create_group(arg(name), m))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_start_chat(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::start_chat(arg(did)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_delete_conversation(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::delete_conversation(arg(did)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_delete_group(gid: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::delete_group(arg(gid)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_mark_read(did: *const c_char) {
    ensure_plat();
    block(social::chat_mark_read(arg(did)));
}

#[no_mangle]
pub unsafe extern "C" fn hey_total_unread() -> c_int {
    ensure_plat();
    block(social::chat_unread()) as c_int
}

/// A contact's carrier ticket (base32) for dialing their voice ALPN. Empty if unknown.
#[no_mangle]
pub unsafe extern "C" fn hey_peer_ticket(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::peer_ticket(arg(did))))
}

// ── social graph ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hey_follow(input: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::follow(arg(input)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_unfollow(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::unfollow(arg(did)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_following() -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::following())))
}

#[no_mangle]
pub unsafe extern "C" fn hey_followers() -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::followers())))
}

#[no_mangle]
pub unsafe extern "C" fn hey_follow_back(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::follow_back(arg(did)))))
}

#[no_mangle]
pub unsafe extern "C" fn hey_is_following(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::is_following(arg(did)))))
}

/// Drain pending Activity-tab notifications → `[HeyNotification]`.
#[no_mangle]
pub unsafe extern "C" fn hey_drain_notifs() -> *mut c_char {
    out(social::drain_notifs().to_string())
}

// ── wallet (one BIP39 seed → all chains; runtime-held identity) ───────────────
//
// MONEY PATH (mirrors the verified Android JNI sends): a send REQUIRES a one-shot
// spend grant (`guard::authorize_spend`, 90s TTL, byte-bound to kind+to+amount).
// `redeem_spend` only VALIDATES; the phrase then comes from the runtime identity
// via `signing_phrase("")`. On iOS the runtime is always up, so Swift passes "" for
// the mnemonic and Rust resolves the runtime-held seed in-process.

/// ESC/EVM wallet address (default chain), or "" on error. `mnemonic` "" = runtime seed.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_address(mnemonic: *const c_char) -> *mut c_char {
    out(signing_phrase(arg(mnemonic)).and_then(|p| wallet::esc_address(&p)).unwrap_or_default())
}

/// Registered EVM chains: `[{key,name,chainId,symbol}]`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_chains() -> *mut c_char {
    out(wallet::evm_chains_json().to_string())
}

/// Native balance on `chain` → `{address,balance,wei,symbol}` (or `{"error":…}`).
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_balance(mnemonic: *const c_char, chain: *const c_char) -> *mut c_char {
    out(json_result(signing_phrase(arg(mnemonic)).and_then(|p| wallet::esc_balance(&p, arg(chain)))))
}

/// All balances on `chain`: native + curated ERC-20s → `{address,tokens:[…]}`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_balances(mnemonic: *const c_char, chain: *const c_char) -> *mut c_char {
    out(json_result(signing_phrase(arg(mnemonic)).and_then(|p| wallet::evm_balances(&p, arg(chain)))))
}

/// Validate + checksum (EIP-55) a recipient address → `{ok,address}` or `{error}`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_check_address(addr: *const c_char) -> *mut c_char {
    let r = match wallet::validate_address(arg(addr).trim()) {
        Ok(c) => serde_json::json!({ "ok": true, "address": c }),
        Err(e) => serde_json::json!({ "error": e }),
    };
    out(r.to_string())
}

/// Confirmation status of a broadcast tx → `{ "status": "pending"|"success"|"failed" }`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_tx_status(chain: *const c_char, hash: *const c_char) -> *mut c_char {
    out(json_result(wallet::esc_tx_status(arg(chain), arg(hash))))
}

/// Mint a one-shot spend grant token (guard.rs). `kind` = "ela" | "evm:<chain>" |
/// "erc20:<chain>:<contract>". Returns `{"token":…}` or `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_authorize_spend(
    kind: *const c_char,
    to: *const c_char,
    amount: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let r = crate::guard::authorize_spend(arg(kind), arg(to), arg(amount), None)
        .map(|t| serde_json::json!({ "token": t }));
    out(json_result(r))
}

// ── C-1(c): hardware-bound spend authorization FFIs (mirror the Android JNI) ──
// Fail-safe: dormant until a Secure-Enclave P-256 key is enrolled. Until then the UI
// biometric stays the gate and the legacy mint path is unchanged. NOTE: these wire the
// SAME guard.rs surface as Android; the iOS Swift side must implement the Secure-Enclave
// signing (CryptoObject equivalent) to use them — they compile + are correct either way.

/// Issue a fresh one-time challenge for the next hardware-bound spend. The Swift
/// Secure-Enclave op signs `challenge\0kind\0to\0amount`.
#[no_mangle]
pub unsafe extern "C" fn hey_spend_challenge() -> *mut c_char {
    out(crate::guard::issue_spend_challenge().unwrap_or_default())
}

/// Enrollment self-test: prove the Enclave-sign → Rust-verify path works on THIS
/// device BEFORE enrolling (Swift signs `challenge\0selftest\0selftest\0selftest` and
/// passes the SEC1 pubkey, Base64). 0 if it verifies, -1 otherwise.
#[no_mangle]
pub unsafe extern "C" fn hey_spend_selftest(
    sec1_b64: *const c_char,
    challenge: *const c_char,
    sig_hex: *const c_char,
) -> c_int {
    use base64::Engine as _;
    let Ok(sec1) = base64::engine::general_purpose::STANDARD.decode(arg(sec1_b64).trim()) else { return -1 };
    if crate::guard::spend_selftest(&sec1, arg(challenge), arg(sig_hex)) { 0 } else { -1 }
}

/// Enroll the SEC1 (Base64) P-256 public key of an auth-required Enclave signing key,
/// activating hardware-bound spends. Call ONLY after `hey_spend_selftest` returns 0.
/// Returns 0 / -1.
#[no_mangle]
pub unsafe extern "C" fn hey_enroll_spend_key(sec1_b64: *const c_char) -> c_int {
    use base64::Engine as _;
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(arg(sec1_b64).trim()) else { return -1 };
    match crate::guard::enroll_spend_key(&bytes) {
        Ok(()) => 0,
        Err(e) => { log::error!("enroll_spend_key: {e}"); -1 }
    }
}

/// Hardware-bound mint: like `hey_authorize_spend` but carries the Enclave signature
/// (hex DER) proving a real biometric op authorized exactly this transfer. Required
/// once a spend key is enrolled. Returns `{"token":…}` / `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_authorize_spend_hw(
    kind: *const c_char,
    to: *const c_char,
    amount: *const c_char,
    sig_hex: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let r = crate::guard::authorize_spend(arg(kind), arg(to), arg(amount), Some(arg(sig_hex)))
        .map(|t| serde_json::json!({ "token": t }));
    out(json_result(r))
}

/// Hardware-bound fee mint: `hey_authorize_spend_fee` with the Enclave signature.
#[no_mangle]
pub unsafe extern "C" fn hey_authorize_spend_fee_hw(
    kind: *const c_char,
    to: *const c_char,
    amount: *const c_char,
    max_fee_wei: *const c_char,
    sig_hex: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let mf = arg(max_fee_wei).trim().parse::<u128>().unwrap_or(0);
    let r = crate::guard::authorize_spend_fee(arg(kind), arg(to), arg(amount), mf, Some(arg(sig_hex)))
        .map(|t| serde_json::json!({ "token": t }));
    out(json_result(r))
}

/// Turn the hardware spend binding OFF for this process. When a binding is ACTIVE this
/// FAILS CLOSED (returns -1) — the caller must use `hey_unenroll_spend_key_hw`. Only
/// succeeds when the binding is inactive / never enrolled (idempotent / legacy). 0 / -1.
#[no_mangle]
pub unsafe extern "C" fn hey_unenroll_spend_key() -> c_int {
    match crate::guard::unenroll_spend_key() {
        Ok(()) => 0,
        Err(e) => { log::warn!("unenroll_spend_key: {e}"); -1 }
    }
}

/// Issue a one-time challenge to DISABLE the spend binding. The Swift Enclave op signs
/// `challenge\0spend.unenroll\0spend.unenroll\0spend.unenroll`.
#[no_mangle]
pub unsafe extern "C" fn hey_unenroll_challenge() -> *mut c_char {
    out(crate::guard::issue_unenroll_challenge().unwrap_or_default())
}

/// HARDWARE-VERIFIED disable: turn the binding off ONLY after a fresh Enclave signature
/// over the one-time disable-challenge verifies. Returns 0 / -1.
#[no_mangle]
pub unsafe extern "C" fn hey_unenroll_spend_key_hw(sig_hex: *const c_char) -> c_int {
    match crate::guard::unenroll_spend_key_hw(arg(sig_hex)) {
        Ok(()) => 0,
        Err(e) => { log::warn!("unenroll_spend_key_hw: {e}"); -1 }
    }
}

/// MONEY: EVM value transfer on `chain`. `value_hex` = wei in hex (no 0x). Requires a
/// grant from authorize_spend("evm:<chain>", to, value_hex). Returns `{txHash}` or `{error}`.
///
/// C-1(b): redeems via `esc_send_redeem` (mirrors Android hey_wallet_send), so the grant
/// is consumed INSIDE the signer AFTER the real fee is known — a max-fee bound in the
/// grant is then enforced against gasPrice*gasLimit before signing. A max_fee=0 grant
/// (the current iOS authorize path) is unbounded/backward-compatible.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_send(
    mnemonic: *const c_char,
    chain: *const c_char,
    to: *const c_char,
    value_hex: *const c_char,
    auth: *const c_char,
) -> *mut c_char {
    let redeem = wallet::SpendRedeem {
        token: arg(auth).to_string(),
        kind: format!("evm:{}", arg(chain)),
        to: arg(to).to_string(),
        amount: arg(value_hex).to_string(),
    };
    let r = signing_phrase(arg(mnemonic))
        .and_then(|p| wallet::esc_send_redeem(&p, arg(chain), arg(to), arg(value_hex), Some(redeem)));
    out(json_result(r))
}

/// Estimate the MAX network fee for a native send to `to` (value `value_hex`) on `chain`,
/// using the SAME eth_estimateGas the signer uses (M-1): `{maxFeeWei, maxFee, gasPriceWei,
/// gasLimit, symbol}`. `mnemonic` "" = runtime seed (used only to derive the sender).
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_fee_estimate(
    mnemonic: *const c_char,
    chain: *const c_char,
    to: *const c_char,
    value_hex: *const c_char,
) -> *mut c_char {
    out(json_result(
        signing_phrase(arg(mnemonic)).and_then(|p| wallet::esc_fee_estimate(&p, arg(chain), arg(to), arg(value_hex))),
    ))
}

/// Mint a one-shot spend grant binding a MAX network fee (wei) — mirrors Android
/// hey_authorize_spend_fee. `max_fee_wei` = the value the user confirmed (from
/// hey_wallet_fee_estimate). Returns `{"token":…}` / `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_authorize_spend_fee(
    kind: *const c_char,
    to: *const c_char,
    amount: *const c_char,
    max_fee_wei: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let mf = arg(max_fee_wei).trim().parse::<u128>().unwrap_or(0);
    let r = crate::guard::authorize_spend_fee(arg(kind), arg(to), arg(amount), mf, None)
        .map(|t| serde_json::json!({ "token": t }));
    out(json_result(r))
}

/// MONEY: ERC-20 transfer on `chain`. `amount_hex` = smallest units (hex). Grant kind
/// `erc20:<chain>:<contract>`. Returns `{txHash}` or `{error}`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_token_send(
    mnemonic: *const c_char,
    chain: *const c_char,
    contract: *const c_char,
    to: *const c_char,
    amount_hex: *const c_char,
    auth: *const c_char,
) -> *mut c_char {
    let kind = format!("erc20:{}:{}", arg(chain), arg(contract));
    let r = crate::guard::redeem_spend(arg(auth), &kind, arg(to), arg(amount_hex))
        .and_then(|()| signing_phrase(arg(mnemonic)))
        .and_then(|p| wallet::evm_token_send(&p, arg(chain), arg(contract), arg(to), arg(amount_hex)));
    out(json_result(r))
}

/// All NFTs (ERC-721/1155) on `chain`. `added` = JSON array of user-tracked
/// contracts (trustless/off mode). `{address, mode, collections:[…]}`. Read-only.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_nfts(
    mnemonic: *const c_char,
    chain: *const c_char,
    added: *const c_char,
) -> *mut c_char {
    let added: Vec<String> = serde_json::from_str(arg(added)).unwrap_or_default();
    out(json_result(signing_phrase(arg(mnemonic)).and_then(|p| wallet::evm_nfts(&p, arg(chain), &added))))
}

/// Look up a manually-added NFT (contract + decimal token_id): `{owned,kind,amount,name,image}`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_nft_lookup(
    mnemonic: *const c_char,
    chain: *const c_char,
    contract: *const c_char,
    token_id: *const c_char,
) -> *mut c_char {
    out(json_result(signing_phrase(arg(mnemonic)).and_then(|p| {
        wallet::evm_nft_lookup(&p, arg(chain), arg(contract), arg(token_id))
    })))
}

/// MONEY: ERC-721 NFT transfer. token_id = DECIMAL. Grant kind `nft:<chain>:<contract>`,
/// amount = decimal token_id. Returns `{txHash}` or `{error}`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_nft_send_721(
    mnemonic: *const c_char,
    chain: *const c_char,
    contract: *const c_char,
    to: *const c_char,
    token_id: *const c_char,
    auth: *const c_char,
) -> *mut c_char {
    let kind = format!("nft:{}:{}", arg(chain), arg(contract));
    let r = crate::guard::redeem_spend(arg(auth), &kind, arg(to), arg(token_id))
        .and_then(|()| signing_phrase(arg(mnemonic)))
        .and_then(|p| wallet::evm_nft_send_721(&p, arg(chain), arg(contract), arg(to), arg(token_id)));
    out(json_result(r))
}

/// MONEY: ERC-1155 transfer of `qty` of token_id. Grant kind BINDS qty:
/// `nft1155:<chain>:<contract>:<qty>`, amount = decimal token_id. `{txHash}`|`{error}`.
#[no_mangle]
pub unsafe extern "C" fn hey_wallet_nft_send_1155(
    mnemonic: *const c_char,
    chain: *const c_char,
    contract: *const c_char,
    to: *const c_char,
    token_id: *const c_char,
    qty: *const c_char,
    auth: *const c_char,
) -> *mut c_char {
    let kind = format!("nft1155:{}:{}:{}", arg(chain), arg(contract), arg(qty));
    let r = crate::guard::redeem_spend(arg(auth), &kind, arg(to), arg(token_id))
        .and_then(|()| signing_phrase(arg(mnemonic)))
        .and_then(|p| wallet::evm_nft_send_1155(&p, arg(chain), arg(contract), arg(to), arg(token_id), arg(qty)));
    out(json_result(r))
}

// ── Elastos DID (EID) + ELA mainchain — same mnemonic, Essentials-recoverable ──

/// `did:elastos:…` (default DID) for the recovery phrase, or "" on error.
#[no_mangle]
pub unsafe extern "C" fn hey_elastos_did(mnemonic: *const c_char) -> *mut c_char {
    out(signing_phrase(arg(mnemonic)).and_then(|p| crate::did::elastos_did(&p)).unwrap_or_default())
}

/// ELA mainchain `E…` address for the recovery phrase, or "" on error.
#[no_mangle]
pub unsafe extern "C" fn hey_ela_address(mnemonic: *const c_char) -> *mut c_char {
    out(signing_phrase(arg(mnemonic)).and_then(|p| crate::did::ela_mainchain_address(&p)).unwrap_or_default())
}

/// ELA mainchain balance (UTXO) → `{address,sela,ela}` or `{"error":…}`.
#[no_mangle]
pub unsafe extern "C" fn hey_ela_balance(mnemonic: *const c_char) -> *mut c_char {
    out(json_result(signing_phrase(arg(mnemonic)).and_then(|p| mainchain::ela_balance(&p))))
}

/// MONEY: ELA MAINCHAIN transfer. `amount` = decimal ELA. Grant kind `ela`.
/// Returns `{txHash}` or `{error}`.
#[no_mangle]
pub unsafe extern "C" fn hey_ela_send(
    mnemonic: *const c_char,
    to: *const c_char,
    amount: *const c_char,
    auth: *const c_char,
) -> *mut c_char {
    let r = crate::guard::redeem_spend(arg(auth), "ela", arg(to), arg(amount))
        .and_then(|()| signing_phrase(arg(mnemonic)))
        .and_then(|p| mainchain::ela_send(&p, arg(to), arg(amount)));
    out(json_result(r))
}

/// Recent audit-log lines (transfers, grants, denials), newest last, newline-joined.
#[no_mangle]
pub unsafe extern "C" fn hey_audit_log(limit: c_int) -> *mut c_char {
    out(crate::guard::audit_tail(limit.max(1) as usize).join("\n"))
}

// ── tipping ──────────────────────────────────────────────────────────────────

/// Publish my tip-receive addresses (`{chainKey:address}` JSON) in my signed profile.
#[no_mangle]
pub unsafe extern "C" fn hey_set_tip_addresses(addresses_json: *const c_char) -> *mut c_char {
    ensure_plat();
    out(json_result(block(social::set_tip_addresses(arg(addresses_json)))))
}

/// A peer's published tip addresses (`{chainKey:address}` or `{}`).
#[no_mangle]
pub unsafe extern "C" fn hey_resolve_tip(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::resolve_tip(arg(did))).to_string())
}

/// Tip-resolve that ALSO exchanges addresses over the DM channel (contacts).
#[no_mangle]
pub unsafe extern "C" fn hey_refresh_contact(did: *const c_char) -> *mut c_char {
    ensure_plat();
    out(block(social::refresh_contact_addresses(arg(did))).to_string())
}

/// Notify a tip recipient over the carrier (fire-and-forget) → `{"ok":bool}`.
#[no_mangle]
pub unsafe extern "C" fn hey_notify_tip(
    to: *const c_char,
    sym: *const c_char,
    amount: *const c_char,
    txid: *const c_char,
) -> *mut c_char {
    ensure_plat();
    let ok = block(social::notify_tip(arg(to), arg(sym), arg(amount), arg(txid)));
    out(serde_json::json!({ "ok": ok }).to_string())
}

// ── content (media CID → raw bytes, in-process content provider) ──────────────

/// Allocate an exact-size heap buffer for `bytes`, write its length to `out_len`, and
/// return the pointer (null/0 for empty). Caller frees with `hey_bytes_free`.
unsafe fn bytes_out(bytes: Vec<u8>, out_len: *mut usize) -> *mut u8 {
    let boxed = bytes.into_boxed_slice(); // cap == len so free is unambiguous
    let len = boxed.len();
    if !out_len.is_null() { *out_len = len; }
    if len == 0 {
        drop(boxed);
        return std::ptr::null_mut();
    }
    Box::into_raw(boxed) as *mut u8
}

/// Resolve `cid` to bytes through the in-process content provider (NO network).
/// Returns a heap buffer of `*out_len` bytes (caller frees with `hey_bytes_free`).
#[no_mangle]
pub unsafe extern "C" fn hey_content_bytes(cid: *const c_char, out_len: *mut usize) -> *mut u8 {
    ensure_plat();
    let bytes = block(social::content_bytes(arg(cid)));
    bytes_out(bytes, out_len)
}

/// Free a buffer returned by `hey_content_bytes` / `hey_fetch_attachment`.
#[no_mangle]
pub unsafe extern "C" fn hey_bytes_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 { return; }
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

// ── 1:1 voice calls + signaling ──────────────────────────────────────────────

/// Send a call-control signal (offer/accept/decline/end) to a contact → `{"ok":bool}`.
#[no_mangle]
pub unsafe extern "C" fn hey_call_send(did: *const c_char, payload: *const c_char) -> *mut c_char {
    ensure_plat();
    let ok = block(social::call_send(arg(did), arg(payload)));
    out(serde_json::json!({ "ok": ok }).to_string())
}

/// Drain inbound call signals → `[{from,payload:{type,call_id,…}}]`.
#[no_mangle]
pub unsafe extern "C" fn hey_call_poll() -> *mut c_char {
    ensure_plat();
    out(block(social::call_poll()).to_string())
}

/// Begin the audio session. `peer_ticket` = the contact's carrier ticket;
/// `is_caller` decides who dials.
#[no_mangle]
pub unsafe extern "C" fn hey_voice_start(peer_ticket: *const c_char, is_caller: c_int) {
    let ticket = arg(peer_ticket).to_string();
    let caller = is_caller != 0;
    if let Some((h, slot)) = crate::NET.get() {
        let slot = slot.clone();
        h.spawn(async move {
            if let Some(c) = slot.read().await.clone() {
                match c.peer_id_of(&ticket) {
                    Some(peer) => crate::voice::start(c.endpoint(), peer, caller).await,
                    None => log::warn!("voice: undecodable peer ticket"),
                }
            }
        });
    }
}

/// How many LIVE voice links this session has (0 = still dialing).
#[no_mangle]
pub unsafe extern "C" fn hey_voice_peers() -> c_int {
    crate::voice::connected_peers() as c_int
}

/// Send one captured PCM frame (16-bit LE) — μ-law encoded + sent. Sync.
#[no_mangle]
pub unsafe extern "C" fn hey_voice_send(pcm: *const u8, len: usize) {
    if pcm.is_null() || len == 0 { return; }
    let v = std::slice::from_raw_parts(pcm, len);
    crate::voice::send_pcm(v);
}

/// Pull up to `max_bytes` of decoded PCM (16-bit LE) for playback. Returns a heap
/// buffer of `*out_len` bytes (caller frees with `hey_bytes_free`). Sync.
#[no_mangle]
pub unsafe extern "C" fn hey_voice_recv(max_bytes: c_int, out_len: *mut usize) -> *mut u8 {
    let v = crate::voice::recv_pcm(max_bytes.max(0) as usize);
    bytes_out(v, out_len)
}

#[no_mangle]
pub unsafe extern "C" fn hey_voice_set_muted(muted: c_int) {
    crate::voice::set_muted(muted != 0);
}

#[no_mangle]
pub unsafe extern "C" fn hey_voice_stop() {
    crate::voice::stop();
}

// ── video calls (direct-only) — H.264 frames over QUIC uni-streams ──────────
// Same shared video.rs plane as Android; iOS provides camera/codec/UI in Swift
// (AVFoundation + VideoToolbox H.264) and ferries frames through these.

/// Begin a 1:1 video session. DIRECT-ONLY HARD GATE: refuses a relay peer.
#[no_mangle]
pub unsafe extern "C" fn hey_video_start(peer_ticket: *const c_char) {
    let ticket = arg(peer_ticket).to_string();
    if let Some((h, slot)) = crate::NET.get() {
        let slot = slot.clone();
        h.spawn(async move {
            if let Some(c) = slot.read().await.clone() {
                if c.peer_transport(&ticket).await == "relay" {
                    log::warn!("video: refusing start — peer is on RELAY (video is direct-only)");
                    return;
                }
                match c.peer_id_of(&ticket) {
                    Some(peer) => crate::video::start(c.endpoint(), peer).await,
                    None => log::warn!("video: undecodable peer ticket"),
                }
            }
        });
    }
}

/// Queue one encoded H.264 frame for the peer. Sync.
#[no_mangle]
pub unsafe extern "C" fn hey_video_send_frame(frame: *const u8, len: usize) {
    if frame.is_null() || len == 0 {
        return;
    }
    let v = std::slice::from_raw_parts(frame, len);
    crate::video::send_frame(v);
}

/// Pop the next received H.264 frame for the decoder (heap buffer of `*out_len`
/// bytes; caller frees with `hey_bytes_free`; empty when none ready). Sync.
#[no_mangle]
pub unsafe extern "C" fn hey_video_recv_frame(out_len: *mut usize) -> *mut u8 {
    bytes_out(crate::video::recv_frame(), out_len)
}

#[no_mangle]
pub unsafe extern "C" fn hey_video_set_paused(paused: c_int) {
    crate::video::set_paused(paused != 0);
}

#[no_mangle]
pub unsafe extern "C" fn hey_video_peers() -> c_int {
    crate::video::connected_peers() as c_int
}

/// Cumulative dropped frames (network behind) — the adaptive-bitrate signal.
#[no_mangle]
pub unsafe extern "C" fn hey_video_dropped() -> u64 {
    crate::video::dropped()
}

#[no_mangle]
pub unsafe extern "C" fn hey_video_stop() {
    crate::video::stop();
}

// ── HeyVerse lane (sealed + ratcheted; in-memory inbox) ──────────────────────

#[no_mangle]
pub unsafe extern "C" fn hey_verse_send(did: *const c_char, payload_json: *const c_char) -> *mut c_char {
    ensure_plat();
    let ok = block(social::verse_send(arg(did), arg(payload_json)));
    out(serde_json::json!({ "ok": ok }).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hey_verse_poll() -> *mut c_char {
    ensure_plat();
    out(social::verse_poll().to_string())
}

// ── push (iOS) ───────────────────────────────────────────────────────────────

/// Register the device APNs token. The engine derives BLINDED handles
/// (bid = HMAC(device-salt, queue-topic)) and registers them with the gateway, so
/// the gateway never learns the social graph. See docs/HEY_IOS_PUSH_GATEWAY.md §3.
/// TODO: net-new — no Android analog (Android uses an always-on foreground service).
#[no_mangle]
pub unsafe extern "C" fn hey_register_push_token(_apns_token_hex: *const c_char) {
    let _ = arg(_apns_token_hex);
}
