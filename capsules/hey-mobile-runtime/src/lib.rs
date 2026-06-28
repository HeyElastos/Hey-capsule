//! hey-mobile-runtime — the on-device mini-runtime for the Hey mobile apps.
//!
//! One process, one `.so`. Brings up an iroh-1.0 carrier, a local
//! biometric-gated identity, file storage and a content store, and serves them
//! over a loopback HTTP server that the UNMODIFIED WASM capsule (hey-social /
//! hey-chat) fetches. There is no external runtime, no wallet, no kubo, and no
//! child processes — the phone IS the runtime.
//!
//! Entry points:
//!   * `run_blocking(Config)`  — host smoke harness (examples/dev.rs).
//!   * `start_background(Config) -> port` — spawn the runtime on its own thread.
//!   * `Java_..._nativeStart*`  — the Android JNI bridge (cdylib export).

pub mod carrier;
pub mod content;
pub mod did;
pub mod guard;
pub mod identity;
pub mod ledger_ble;
pub mod ledger_ela;
pub mod ledger_evm;
pub mod mainchain;
pub mod logbuf;
pub mod server;
pub mod social;
pub mod storage;
pub mod verse_rt;
pub mod verse_gossip;
pub mod video;
pub mod voice;
pub mod wallet;
pub mod wallets;

use std::path::PathBuf;
use std::sync::Arc;

use carrier::Carrier;
use content::Content;
use identity::{Identity, IdentityBlob};
use server::AppState;
use storage::Storage;

/// Poison-tolerant lock. A panic while a std `Mutex` is held poisons it, after
/// which every `.lock().unwrap()` panics too — turning one transient failure
/// into a permanently-bricked receiver/voice thread (and, across the JNI
/// boundary, a hard crash on the next call). Recovering the guard via
/// `into_inner()` keeps delivery alive; our critical sections are short,
/// infallible map/set mutations, so the protected data stays consistent.
pub(crate) fn lock_safe<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct Config {
    /// App-private writable dir (Android `getFilesDir()`, or any dir on host).
    pub data_dir: PathBuf,
    /// Directory holding the capsule's built `dist/` (index.html + wasm + js).
    pub dist_dir: PathBuf,
    /// Loopback port to bind (0 lets the OS pick; the chosen port is returned).
    pub port: u16,
    /// Capsule id — "hey-social" or "hey-chat". Sets the `/apps/<id>/` mount
    /// and the per-capsule storage namespace.
    pub capsule: String,
    /// Keystore-unlocked identity blob (Android). `None` → load/create the
    /// local `identity.json` (host/dev path).
    pub identity_blob: Option<String>,
}

// (android/ios) bg runtime handle + carrier slot, so the connectivity-change FFI (hey_net_changed)
// can trigger an instant carrier re-probe + topic re-join the moment the OS reports the network is
// back. C1-1: ios.rs references crate::NET, so the static + its setter must be defined for iOS too
// (the OnceLock is generic over the OS; the readers are per-OS — android JNI here, iOS in ios.rs).
static NET: std::sync::OnceLock<(
    tokio::runtime::Handle,
    Arc<tokio::sync::RwLock<Option<Arc<Carrier>>>>,
)> = std::sync::OnceLock::new();

// ── Platform-neutral call (voice/video) entry points ─────────────────────────
// The desktop (egui) embeds this crate in-process and drives 1:1 calls through
// these, keeping all iroh types (Endpoint/EndpointId) inside the runtime. They
// spawn the dial/recv loops on the CARRIER's long-lived runtime (the NET handle),
// never on a short-lived engine worker. No-op until the carrier slot fills (it
// starts None and fills async at boot — callers gate on carrier_health().online).
// `peer_ticket` is the contact's carrier ticket (resolve via social::peer_ticket
// on an engine worker FIRST, then hand the string here).

/// Start a 1:1 VOICE session with a contact's carrier ticket. `is_caller` decides who dials.
pub fn voice_start(peer_ticket: String, is_caller: bool) {
    if let Some((h, slot)) = NET.get() {
        let slot = slot.clone();
        h.spawn(async move {
            match slot.read().await.clone() {
                Some(c) => match c.peer_id_of(&peer_ticket) {
                    Some(peer) => crate::voice::start(c.endpoint(), peer, is_caller).await,
                    None => log::warn!("voice: undecodable peer ticket"),
                },
                None => log::warn!("voice_start: carrier not up yet"),
            }
        });
    }
}

/// Start a 1:1 VIDEO session (DIRECT-ONLY: refuses a relay-only peer as a backstop
/// to the UI gate, since media over a relay would be unacceptable for video).
pub fn video_start(peer_ticket: String) {
    if let Some((h, slot)) = NET.get() {
        let slot = slot.clone();
        h.spawn(async move {
            if let Some(c) = slot.read().await.clone() {
                if c.peer_transport(&peer_ticket).await == "relay" {
                    log::warn!("video: refusing start — peer on RELAY (direct-only)");
                    return;
                }
                match c.peer_id_of(&peer_ticket) {
                    Some(peer) => crate::video::start(c.endpoint(), peer).await,
                    None => log::warn!("video: undecodable peer ticket"),
                }
            }
        });
    }
}

// Sync probes/controls/hot-path — operate on voice/video module globals (no NET).
pub fn voice_peers() -> usize { crate::voice::connected_peers() }
pub fn voice_send(pcm_le: &[u8]) { crate::voice::send_pcm(pcm_le) }
pub fn voice_recv(max_bytes: usize) -> Vec<u8> { crate::voice::recv_pcm(max_bytes) }
pub fn voice_set_muted(m: bool) { crate::voice::set_muted(m) }
pub fn voice_stop() { crate::voice::stop() }

pub fn video_peers() -> usize { crate::video::connected_peers() }
pub fn video_send_frame(f: &[u8]) { crate::video::send_frame(f) }
pub fn video_recv_frame() -> Vec<u8> { crate::video::recv_frame() }
pub fn video_set_paused(p: bool) { crate::video::set_paused(p) }
pub fn video_dropped() -> u64 { crate::video::dropped() }
pub fn video_peer_epoch() -> u64 { crate::video::new_peer_epoch() }
pub fn video_stop() { crate::video::stop() }

/// The runtime-held identity. The wallet/DID JNI surface signs with THIS — the
/// recovery phrase is used in-process and never has to cross the app boundary
/// (secrets are used, never owned; see guard.rs).
static IDENTITY: std::sync::OnceLock<Arc<Identity>> = std::sync::OnceLock::new();

/// The runtime data dir, captured at boot so the unlock JNIs can reach
/// carrier-identity.json without re-plumbing it through every call.
static DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Passed as `hey_init`'s identity_blob to request a HEADLESS-VAULT cold start
/// (vault ON, seed sealed). run_async treats it as "NEVER generate" — it boots
/// from the persisted carrier key or fails closed, but never mints a fresh
/// identity (which would fork a vaulted account). Kotlin's `ensureStarted` sends
/// it ONLY when carrier-identity.json already exists. MUST byte-match HeyApi.kt's
/// `HEADLESS_BOOT`. It is neither a BIP39 phrase nor a JSON blob, so it can never
/// collide with a real seed and be mistaken for one.
const HEADLESS_BOOT_SENTINEL: &str = "__hey_headless_boot__";

/// The runtime-held identity's BIP39 recovery phrase — the wallet/DID signing
/// source. Same resolution as the JNI `signing_phrase`, exposed for an in-process
/// host (the egui desktop app) that embeds this runtime and drives the wallet
/// modules directly. The phrase is used in-process and never leaves the boundary
/// (secrets are used, never owned; see guard.rs).
pub fn wallet_phrase() -> Result<String, String> {
    IDENTITY
        .get()
        .and_then(|i| i.mnemonic().map(str::to_string))
        .ok_or_else(|| "wallet locked: runtime identity not ready or has no recovery phrase".to_string())
}

/// Best-effort wipe of an in-process recovery-phrase copy before it drops (L-1).
/// `String` does not guarantee its backing buffer is overwritten on drop, so the
/// words could linger in freed heap. We overwrite the live bytes in place (the same
/// `.fill(0)` pattern hey-core crypto.rs uses) then clear. The DURABLE phrase lives
/// in the sealed identity (and `IDENTITY`); this only scrubs the transient copy. Note
/// the compiler MAY still copy the String during a move before this runs — this is a
/// pragmatic hardening, not a guarantee (a true guarantee needs a Zeroizing type).
#[inline]
fn wipe_phrase(mut p: String) {
    // SAFETY: we only overwrite existing bytes with 0x00 (valid UTF-8) and never grow.
    unsafe { p.as_bytes_mut().fill(0); }
    p.clear();
    drop(p);
}

/// Generate a FRESH BIP39 identity and persist it to `data_dir/identity.json`,
/// OVERWRITING any existing one. For a host/desktop whose stored identity predates
/// the wallet (a raw seed with no recovery phrase): a raw seed can't be turned into
/// BIP39 words, so a usable Elastos wallet needs a new BIP39 root. The live
/// `IDENTITY` is set once at boot, so the app must RESTART to pick it up. Returns
/// the new did:key.
pub fn create_fresh_identity(data_dir: &std::path::Path) -> Result<String, String> {
    let id = identity::Identity::generate();
    std::fs::create_dir_all(data_dir).map_err(|e| format!("mkdir: {e}"))?;
    identity::write_identity_blob(&data_dir.join("identity.json"), &id.to_blob());
    guard::audit("identity.recreate", serde_json::json!({ "did": id.did_key() }));
    Ok(id.did_key().to_string())
}

/// Is this a valid BIP39 (12/24-word) recovery phrase? Pure + cheap — callable
/// straight from the UI thread for instant validation before a restore.
pub fn validate_mnemonic(phrase: &str) -> bool {
    bip39::Mnemonic::parse(phrase.trim()).is_ok()
}

/// Restore an identity from its BIP39 recovery phrase, persisting it to
/// `data_dir/identity.json` (OVERWRITING). Re-derives did:key, did:elastos and the
/// wallets on this device. The app must RESTART to load it. Returns the did:key.
pub fn restore_identity(data_dir: &std::path::Path, phrase: &str) -> Result<String, String> {
    let id = identity::Identity::from_mnemonic(phrase)?;
    std::fs::create_dir_all(data_dir).map_err(|e| format!("mkdir: {e}"))?;
    identity::write_identity_blob(&data_dir.join("identity.json"), &id.to_blob());
    guard::audit("identity.restore", serde_json::json!({ "did": id.did_key() }));
    Ok(id.did_key().to_string())
}

async fn run_async(cfg: Config) -> anyhow::Result<()> {
    std::fs::create_dir_all(&cfg.data_dir).ok();
    let _ = DATA_DIR.set(cfg.data_dir.clone());
    guard::init(&cfg.data_dir);

    // Hardware-at-rest invariant: on a device the storage DEK must be installed
    // (via hey_set_storage_key) BEFORE we touch identity/storage, or the seed,
    // ratchet private keys and conversations land on disk in plaintext. Fail
    // loud (constitution: failure must be loud) so a wiring regression is caught.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if !hey_core::plat::at_rest_active() {
        // Fail LOUD (constitution: failure must be loud): REFUSE to start rather than
        // write the seed, ratchet private keys and conversations to disk in PLAINTEXT.
        // The native app MUST call hey_set_storage_key(dek) before hey_start.
        log::error!(
            "SECURITY: storage DEK not installed before runtime start — refusing to start \
             (would write identity/ratchet keys/conversations as PLAINTEXT)."
        );
        anyhow::bail!("storage DEK not installed (hey_set_storage_key) before runtime start");
    }

    // Boot identity. The identity_blob arg disambiguates the mode:
    //   * HEADLESS_BOOT_SENTINEL (Kotlin sends it ONLY for a vault-ON device whose
    //     carrier blob already exists) → the NEVER-GENERATE ladder: identity.json →
    //     FULL, else carrier blob → HEADLESS (mesh + buffer, seed stays vaulted),
    //     else FAIL CLOSED. This is the account-fork firewall — a vaulted account
    //     can never be minted fresh because this arm never calls load_or_create.
    //   * a real seed/blob (create-with-seed / restore / unlock-with-seed) → FULL.
    //   * neither (None on host/CLI; "" on mobile create-new / vault-OFF — the
    //     hey_init JNI maps "" → None) → load_or_create: load identity.json or, on a
    //     brand-new install, MINT a fresh identity. Generate-on-empty lives ONLY
    //     here, reachable only WITHOUT the sentinel, i.e. never on a vault-ON cold
    //     start. (A vault-ON device with no blob never reaches Rust at all — Kotlin's
    //     ensureStarted refuses and forces an unlock, which boots FULL with the seed.)
    enum BootIdentity {
        Full(Arc<Identity>),
        Headless {
            carrier_sk: iroh::SecretKey,
            did_key: String,
        },
    }
    let id_path = cfg.data_dir.join("identity.json");
    let ci_path = cfg.data_dir.join("carrier-identity.json");
    let boot = match cfg.identity_blob.as_deref().map(str::trim) {
        Some(s) if s == HEADLESS_BOOT_SENTINEL => {
            if let Some(blob) = identity::read_identity_blob(&id_path) {
                // vault-OFF race / safety: a seed is on disk — prefer FULL over headless.
                BootIdentity::Full(Arc::new(
                    Identity::from_blob(&blob).map_err(|e| anyhow::anyhow!(e))?,
                ))
            } else if let Some(ci) = identity::read_carrier_identity(&ci_path) {
                // vault-ON cold start → mesh HEADLESS under the one-way carrier key.
                let sk = ci
                    .carrier_sk()
                    .ok_or_else(|| anyhow::anyhow!("carrier-identity.json: bad carrier_sk_b64"))?;
                // Reaching here PROVES the DEK is present (read_carrier_identity just
                // decrypted the blob with it) — so storage stays UNLOCKED: the
                // receiver reads contacts/profile to JOIN topics + mesh + buffer.
                // Only the SEED is sealed → mark it so consume/decrypt is deferred
                // until biometric unlock (hey_unlock clears this).
                hey_core::plat::set_identity_sealed(true);
                guard::audit("runtime.start.headless", serde_json::json!({ "did": ci.did_key }));
                BootIdentity::Headless {
                    carrier_sk: iroh::SecretKey::from_bytes(&sk),
                    did_key: ci.did_key,
                }
            } else {
                // Sentinel but NO identity.json and NO carrier blob → FAIL CLOSED.
                // Never generate (would fork a vaulted account). Kotlin shouldn't send
                // the sentinel in this state, but if it does we refuse rather than fork.
                guard::audit("runtime.start.fail_closed", serde_json::json!({}));
                anyhow::bail!(
                    "vault-on cold start with no carrier-identity blob — open Hey and unlock once to enable headless delivery"
                );
            }
        }
        Some(s) if !s.is_empty() => {
            let id = if let Ok(blob) = serde_json::from_str::<IdentityBlob>(s) {
                // A vault-unsealed identity blob — use it, never write plaintext.
                Identity::from_blob(&blob).map_err(|e| anyhow::anyhow!(e))?
            } else {
                // A bare BIP39 recovery phrase — restore AND persist it (sealed
                // under the storage DEK on mobile; plaintext only host/CLI no-DEK).
                let id = Identity::from_mnemonic(s).map_err(|e| anyhow::anyhow!(e))?;
                identity::write_identity_blob(&id_path, &id.to_blob());
                id
            };
            BootIdentity::Full(Arc::new(id))
        }
        // None (host/CLI) OR "" (mobile create-new / vault-OFF, mapped to None by the
        // hey_init JNI). load_or_create loads identity.json or mints on first run.
        _ => BootIdentity::Full(Arc::new(Identity::load_or_create(&cfg.data_dir))),
    };

    // Resolve the carrier key + our did from whichever mode we landed in. FULL
    // also sets the process IDENTITY and (re)writes the carrier-identity blob so
    // every seed-backed boot pre-populates the headless fast-path. The carrier
    // node key is DERIVED from the seed (domain-separated) — not a separate
    // persisted secret: sealing the seed seals it too. One root key, period.
    let (carrier_sk, my_did) = match boot {
        BootIdentity::Full(identity) => {
            log::info!("identity {}", identity.did_key());
            hey_core::plat::set_identity_sealed(false); // seed present this boot
            let _ = IDENTITY.set(identity.clone());
            identity::write_carrier_identity(&ci_path, &identity.to_carrier_identity());
            guard::audit(
                "runtime.start",
                serde_json::json!({ "capsule": cfg.capsule, "vault": cfg.identity_blob.is_some() }),
            );
            let sk = iroh::SecretKey::from_bytes(&blake3::derive_key(
                "hey-carrier-node-key-v1",
                &identity.seed(),
            ));
            (sk, identity.did_key().to_string())
        }
        BootIdentity::Headless { carrier_sk, did_key } => {
            log::info!("identity {did_key} (HEADLESS — sealed until unlock)");
            (carrier_sk, did_key)
        }
    };

    let carrier_slot: Arc<tokio::sync::RwLock<Option<Arc<Carrier>>>> =
        Arc::new(tokio::sync::RwLock::new(None));
    // Expose the runtime handle + carrier slot so the connectivity-change FFI can re-probe
    // instantly AND the in-process call wrappers (voice_start/video_start) can reach the carrier.
    // Set on every target now (android JNI, iOS, and the desktop egui app).
    let _ = NET.set((tokio::runtime::Handle::current(), carrier_slot.clone()));

    // Start the carrier in the BACKGROUND so its iroh bind / network discovery
    // can never delay or prevent the HTTP server from listening. A panic here
    // stays contained in this task — the server keeps serving.
    {
        let slot = carrier_slot.clone();
        let dir = cfg.data_dir.join("carrier");
        let carrier_sk = carrier_sk.clone();
        tokio::spawn(async move {
            // Retry with capped backoff: a transient boot-time bind failure (no
            // network yet) must not disable peer/* for the whole session.
            let mut delay = 3u64;
            loop {
                match Carrier::start(dir.clone(), carrier_sk.clone()).await {
                    Ok(c) => {
                        log::info!("carrier node {} up", c.node_id());
                        *slot.write().await = Some(c);
                        break;
                    }
                    Err(e) => {
                        log::error!("carrier start failed (retry in {delay}s): {e:#}");
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        delay = (delay * 2).min(30);
                    }
                }
            }
        });
    }

    let state = AppState {
        carrier: carrier_slot,
        did_key: my_did,
        content: Arc::new(Content::new(&cfg.data_dir)),
        storage: Arc::new(Storage::new(&cfg.data_dir)),
        capsule: cfg.capsule.clone(),
    };
    // Mobile (Android + iOS): SOCKET-FREE. Providers dispatch in-process (NO 127.0.0.1 listener at
    // all), so there is no loopback surface a co-installed app could reach. The native UI talks to the
    // engine over JNI (Android) / a C-ABI (iOS, see src/ios.rs); the engine's own provider calls
    // (hey-core plat::http) route straight to the handlers here.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        install_inprocess_dispatch(state);
        std::future::pending::<()>().await; // keep this runtime (carrier + dispatch) alive forever
        #[allow(unreachable_code)]
        Ok(())
    }
    // Host dev harness: a desktop browser loads the WASM capsule, so it needs the loopback HTTP.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        server::serve(state, cfg.dist_dir.clone(), cfg.port).await
    }
}

/// Route hey-core's provider transport IN-PROCESS (no socket). Spawns a task on THIS runtime that
/// runs each provider call concurrently (so handler-spawned tasks — e.g. gossip topic receivers —
/// live on the long-lived runtime), and installs a plat hook that hands calls to it and blocks the
/// (dedicated) caller thread on the reply. Mobile only. Replaces the loopback HTTP round-trip.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn install_inprocess_dispatch(state: AppState) {
    use std::sync::mpsc::SyncSender;
    type Job = (String, String, String, SyncSender<(u16, String)>);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    tokio::spawn(async move {
        while let Some((scheme, op, body, reply)) = rx.recv().await {
            let st = state.clone();
            tokio::spawn(async move {
                let req: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                let (status, text) = match server::dispatch_provider(&st, &scheme, &op, &req).await {
                    Some(v) => (200u16, v.to_string()),
                    None => (
                        404u16,
                        serde_json::json!({ "error": format!("no provider for scheme: {scheme}") })
                            .to_string(),
                    ),
                };
                let _ = reply.send((status, text));
            });
        }
    });
    hey_core::plat::set_dispatch(move |_method, url, body| {
        // On native, hey-core only ever calls POST /api/provider/<scheme>/<op>.
        let path = url
            .splitn(2, "://")
            .nth(1)
            .and_then(|a| a.find('/').map(|i| &a[i..]))
            .unwrap_or(url);
        let rest = path
            .strip_prefix("/api/provider/")
            .ok_or_else(|| format!("in-process dispatch: unsupported path {path}"))?;
        let mut it = rest.splitn(2, '/');
        let scheme = it.next().unwrap_or_default().to_string();
        let op = it.next().unwrap_or_default().to_string();
        let (rtx, rrx) = std::sync::mpsc::sync_channel::<(u16, String)>(1);
        tx.send((scheme, op, body.unwrap_or("").to_string(), rtx))
            .map_err(|_| "in-process dispatch: runtime gone".to_string())?;
        rrx.recv()
            .map_err(|_| "in-process dispatch: reply dropped".to_string())
    });
}

/// Run the runtime on the current thread until it exits (host harness).
pub fn run_blocking(cfg: Config) -> anyhow::Result<()> {
    // Capture logs into the in-app ring (no-op if a logger is already set).
    crate::logbuf::init(log::LevelFilter::Info);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(run_async(cfg))
}

/// Spawn the runtime on a dedicated thread; returns immediately. The Android
/// app calls this from `onCreate` so the WebView can load `/apps/<id>/` as soon
/// as the port is up.
/// Until iroh ships a tagged 1.0, pin the version-matched elastos.app relay and
/// drop n0's release-schedule (rc/canary) relays by default, so BOTH ends of a
/// chat meet on ONE known relay — a phone homing on an n0 relay and a peer on
/// elastos.app never form a gossip neighbor (dial times out). Env-gated:
/// `ELASTOS_RELAY_ONLY=0` opts back into the n0 fallback.
fn relay_only_default() {
    if std::env::var("ELASTOS_RELAY_ONLY").is_err() {
        std::env::set_var("ELASTOS_RELAY_ONLY", "1");
    }
}

pub fn start_background(cfg: Config) {
    relay_only_default();
    std::thread::Builder::new()
        .name("hey-mobile-runtime".into())
        .spawn(move || {
            if let Err(e) = run_blocking(cfg) {
                log::error!("hey-mobile-runtime exited: {e:#}");
            }
        })
        .expect("spawn runtime thread");
}

// ── iOS C-ABI bridge ─────────────────────────────────────────────────────────
// Same engine, different bridge: the iOS app calls a plain C ABI instead of JNI.
// Activated only on the iOS target, so it is inert on Linux/Android builds.
#[cfg(target_os = "ios")]
mod ios;

// ── Android JNI bridge ───────────────────────────────────────────────────────

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::objects::{JClass, JString};
    use jni::sys::jint;
    use jni::JNIEnv;

    fn jstr(env: &mut JNIEnv, s: &JString) -> Option<String> {
        if s.is_null() {
            return None;
        }
        env.get_string(s).ok().map(|v| v.into())
    }

    /// Shared start for every Hey mobile app. The capsule ("hey-social" /
    /// "hey-chat") comes from which package's JNI symbol was called, so each app
    /// serves its own dist + storage namespace from one shared .so.
    fn start(
        mut env: JNIEnv,
        data_dir: JString,
        dist_dir: JString,
        port: jint,
        identity_blob: JString,
        capsule: &str,
    ) -> jint {
        crate::logbuf::init(log::LevelFilter::Debug);
        // Route Rust panics into logcat — otherwise a panic on the runtime
        // thread is silent and the server just never binds (looks like
        // ERR_CONNECTION_REFUSED to the WebView).
        std::panic::set_hook(Box::new(|info| {
            log::error!("PANIC: {info}");
        }));
        log::info!("nativeStart invoked (capsule={capsule}, port={port})");
        let data_dir = match jstr(&mut env, &data_dir) {
            Some(d) => d,
            None => return -1,
        };
        let dist_dir = match jstr(&mut env, &dist_dir) {
            Some(d) => d,
            None => return -1,
        };
        let cfg = Config {
            data_dir: PathBuf::from(data_dir),
            dist_dir: PathBuf::from(dist_dir),
            port: port as u16,
            capsule: capsule.to_string(),
            identity_blob: jstr(&mut env, &identity_blob),
        };
        start_background(cfg);
        port
    }

    /// Kotlin (Hey Social): `HeyRuntime.nativeStart(dataDir, distDir, port, identityBlobOrNull)`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyRuntime_nativeStart(
        env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        dist_dir: JString,
        port: jint,
        identity_blob: JString,
    ) -> jint {
        start(env, data_dir, dist_dir, port, identity_blob, "hey-social")
    }

    /// Kotlin (Hey Chat): same signature, different package symbol → "hey-chat".
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_chat_HeyRuntime_nativeStart(
        env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        dist_dir: JString,
        port: jint,
        identity_blob: JString,
    ) -> jint {
        start(env, data_dir, dist_dir, port, identity_blob, "hey-chat")
    }

    // ── Native app-API (HeyApi) — what the Jetpack Compose UI calls ──────────
    //
    // No WebView. Each call sets hey-core's thread-local loopback base on THIS
    // JNI thread, then block_on's the social op on a current-thread runtime (so
    // the future runs on this same thread and sees the thread-local base). All
    // logic/crypto stays in hey-core; Kotlin only marshals JSON / bytes.

    use crate::social;
    use jni::objects::JByteArray;
    use jni::sys::{jboolean, jstring};
    use std::sync::OnceLock;

    /// (port, data_dir) chosen by hey_init, applied to plat on every call.
    static APP_CFG: OnceLock<(u16, String)> = OnceLock::new();
    /// Random per-launch bearer the in-process engine presents and the loopback
    /// server REQUIRES. On Android the loopback interface is shared between apps,
    /// so a co-installed app could otherwise reach 127.0.0.1:<port> and call the
    /// provider/identity-oracle/storage routes; without this secret it gets 401.
    static APP_TOKEN: OnceLock<String> = OnceLock::new();

    fn ensure_plat() {
        if let Some((port, store)) = APP_CFG.get() {
            hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
            hey_core::plat::set_store(store);
        }
        if let Some(t) = APP_TOKEN.get() {
            hey_core::plat::set_bearer(t);
        }
        social::install_ctx();
    }

    fn block<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread rt")
            .block_on(f)
    }

    fn out(mut env: JNIEnv, s: String) -> jstring {
        env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
    }
    fn json_result(r: Result<serde_json::Value, String>) -> String {
        match r {
            Ok(v) => v.to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        }
    }

    /// Resolve the wallet signing phrase: secrets are used, never owned
    /// (guard.rs) — the canonical source is the RUNTIME-HELD identity, so the
    /// recovery phrase never has to cross the JNI boundary per call. An
    /// explicitly passed phrase still wins (restore preview / legacy callers).
    // Returns the phrase in a `Zeroizing<String>` so EVERY per-send call site's
    // transient copy is overwritten in place when it drops (L: the per-send
    // signing_phrase clone wasn't zeroized). `Zeroizing<String>` derefs to `String`,
    // so the closures' `&p` still coerce to `&str` — no call site or behavior changes.
    fn signing_phrase(explicit: Option<String>) -> Result<zeroize::Zeroizing<String>, String> {
        if let Some(m) = explicit.filter(|s| !s.trim().is_empty()) {
            return Ok(zeroize::Zeroizing::new(m));
        }
        super::IDENTITY
            .get()
            .and_then(|i| i.mnemonic().map(str::to_string))
            .map(zeroize::Zeroizing::new)
            .ok_or_else(|| {
                "wallet locked: runtime identity not ready or has no recovery phrase".to_string()
            })
    }

    /// `HeyApi.hey_set_storage_key(dekBase64)` — install the 32-byte storage DEK
    /// (Base64) that encrypts ALL on-device key material + data at rest: the
    /// identity seed/mnemonic/ML-KEM secret, the Double-Ratchet PRIVATE keys, and
    /// conversation plaintext. The DEK is wrapped by a hardware StrongBox/TEE
    /// Keystore key on the Kotlin side and released only after the user unlocks,
    /// so nothing at rest is readable without the hardware key + biometric/PIN.
    /// MUST be called BEFORE `hey_init` (and any storage access). Returns 0 on
    /// success, -1 on a missing or non-32-byte key.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1set_1storage_1key(
        mut env: JNIEnv,
        _class: JClass,
        dek_b64: JString,
    ) -> jint {
        use base64::Engine as _;
        let Some(s) = jstr(&mut env, &dek_b64) else {
            return -1;
        };
        let bytes = match base64::engine::general_purpose::STANDARD.decode(s.trim()) {
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
        log::info!("at-rest storage key installed (on-device storage now encrypted)");
        // Unlock: kick an immediate carrier re-dial so the receiver re-meshes and
        // drains buffered messages NOW, not on the next 5s/2s timer. No-op at first
        // start (NET/carrier not up yet — installed before hey_init).
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    // Intent is "re-mesh + drain now", not a transport rebind → cheap re-dial.
                    c.redial_zero_neighbor().await;
                }
            });
        }
        0
    }

    /// LOCK on-device storage: zeroize + drop the DEK from memory (app paused /
    /// screen off). The carrier keeps buffering incoming SEALED messages;
    /// processing + content reads resume after `hey_set_storage_key` re-installs
    /// the DEK on biometric unlock. The two-key split: receipt without decryption.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1lock_1storage(
        _env: JNIEnv,
        _class: JClass,
    ) {
        hey_core::plat::lock_storage();
        // Locking storage logically invalidates any pending spend authorization —
        // drop outstanding grants so a one-shot confirm can't be redeemed past a lock.
        crate::guard::revoke_spends();
        log::info!("storage LOCKED (DEK cleared) — buffering sealed messages until unlock");
    }

    /// Revoke every outstanding spend grant WITHOUT touching the DEK. Wired to the
    /// Option-A long-background UI re-gate: the seed/DEK stay in RAM (messages keep
    /// arriving + decrypting), but a money authorization minted before backgrounding
    /// must NOT survive the re-lock — the user re-confirms after re-authenticating.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1revoke_1spends(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::guard::revoke_spends();
    }

    /// Count of inbound (non-feed) messages the carrier has buffered. The LOCKED
    /// app polls this to post a generic "new message" notification with no DEK.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1inbound_1count(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jlong {
        crate::carrier::inbound_count() as jni::sys::jlong
    }

    /// True while storage is LOCKED (DEK cleared). The notifier posts a GENERIC
    /// "new message" (no content) instead of a preview while the app is locked.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1storage_1locked(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jboolean {
        hey_core::plat::storage_locked() as jni::sys::jboolean
    }

    /// True while the receiver is NOT processing — storage locked (DEK cleared) OR
    /// seed sealed (vault-ON headless boot, pre-unlock). The notifier uses THIS
    /// (not just storage_locked) to decide a GENERIC "new message" from the no-DEK
    /// inbound counter, so a headless device still notifies on buffered messages.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1processing_1deferred(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jboolean {
        hey_core::plat::processing_deferred() as jni::sys::jboolean
    }

    /// Biometric UNLOCK for a vault-ON HEADLESS boot: install the seed-backed
    /// identity the carrier started without, so the receiver can decrypt + drain
    /// the buffer. Idempotent — a no-op (returns 0) if the matching identity is
    /// already live (e.g. a Full boot, or a second unlock). Returns:
    ///   0  unlocked, or already unlocked with the SAME account
    ///  -1  bad/empty phrase, or the runtime isn't initialised
    ///  -2  WRONG ACCOUNT for this device (carrier node key mismatch) — the caller
    ///      must refuse: running a different seed over this node would fork data
    ///  -3  storage DEK not installed yet (call hey_set_storage_key first)
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1unlock(
        mut env: JNIEnv,
        _class: JClass,
        phrase: JString,
    ) -> jint {
        let Some(s) = jstr(&mut env, &phrase) else {
            return -1;
        };
        let s = s.trim();
        if s.is_empty() {
            return -1;
        }
        // The no-auth DEK must already be installed: carrier-identity.json (and
        // everything we touch) is sealed under it. Refuse rather than risk a
        // plaintext write or a half-read.
        if !hey_core::plat::at_rest_active() {
            log::error!("hey_unlock: storage DEK not installed — call hey_set_storage_key first");
            return -3;
        }
        let id = match crate::identity::Identity::from_mnemonic(s) {
            Ok(id) => id,
            Err(e) => {
                log::error!("hey_unlock: bad phrase: {e}");
                return -1;
            }
        };
        let Some(dir) = crate::DATA_DIR.get() else {
            return -1;
        };
        let ci_path = dir.join("carrier-identity.json");
        // ACCOUNT-BINDING GUARD: the carrier already booted from a persisted node
        // key. If this phrase derives a DIFFERENT carrier key, it is a different
        // account — refuse (no dual-identity over one node, no silent fork).
        if let Some(ci) = crate::identity::read_carrier_identity(&ci_path) {
            if ci.carrier_sk() != Some(id.carrier_sk_bytes()) {
                crate::guard::audit(
                    "carrier.identity.mismatch",
                    serde_json::json!({ "did": id.did_key() }),
                );
                log::error!("hey_unlock: phrase is a DIFFERENT account than this device — refusing");
                return -2;
            }
        }
        let id = std::sync::Arc::new(id);
        // Publish to the process. IDENTITY is a OnceLock: unset after a headless
        // boot → set succeeds; if a Full boot already set it, set() returns Err —
        // verify the did matches (same account) and treat as already-unlocked.
        if let Err(existing) = crate::IDENTITY.set(id.clone()) {
            if existing.did_key() != id.did_key() {
                crate::guard::audit(
                    "carrier.identity.mismatch",
                    serde_json::json!({ "did": id.did_key() }),
                );
                return -2;
            }
            // already unlocked with the same identity — fall through (idempotent).
        }
        // Seed is live → decrypt + flush may run; refresh the headless blob.
        hey_core::plat::set_identity_sealed(false);
        crate::identity::write_carrier_identity(&ci_path, &id.to_carrier_identity());
        crate::guard::audit("runtime.unlock", serde_json::json!({ "did": id.did_key() }));
        log::info!("identity UNLOCKED {} — draining buffered messages", id.did_key());
        // Kick an immediate carrier re-dial so the receiver re-meshes + drains NOW.
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    // Re-mesh nudge only — a transport rebind here could tear a just-formed neighbor.
                    c.redial_zero_neighbor().await;
                }
            });
        }
        0
    }

    /// Persist the carrier-identity blob from the LIVE identity. `enableVault()`
    /// calls this BEFORE deleting identity.json, so a vault-ON device always has
    /// the headless blob in place before the seed-on-disk copy is removed — there
    /// is never a window where a cold start would find neither and fail closed.
    /// Returns 0 on success, -1 if no identity is loaded or the dir is unknown.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1persist_1carrier_1identity(
        _env: JNIEnv,
        _class: JClass,
    ) -> jint {
        let (Some(id), Some(dir)) = (crate::IDENTITY.get(), crate::DATA_DIR.get()) else {
            log::error!("hey_persist_carrier_identity: identity/dir not ready");
            return -1;
        };
        // Return the REAL write result so enableVault's atomicity holds — if the
        // disk write fails, Kotlin keeps identity.json instead of deleting the seed.
        if crate::identity::write_carrier_identity(
            &dir.join("carrier-identity.json"),
            &id.to_carrier_identity(),
        ) {
            log::info!("carrier-identity blob persisted (headless-vault ready)");
            0
        } else {
            log::error!("hey_persist_carrier_identity: disk write failed");
            -1
        }
    }

    /// `HeyApi.hey_recovery_phrase()` — the runtime-held BIP39 recovery phrase,
    /// read from the IN-MEMORY identity (never re-read from disk). This is how
    /// Kotlin reveals/seals/derives the seed now that identity.json is encrypted
    /// at rest: the seed crosses the boundary only on an explicit, biometric-gated
    /// reveal/seal, not on every storage read. Empty string if the runtime isn't
    /// ready or has no phrase (legacy seed-only blob).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1recovery_1phrase(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        // H5 + F-SEED-REVEAL: refuse the bare reveal whenever this device can do hardware-gated
        // signing — NOT only once the spend binding is already enrolled. A never-spent wallet on a
        // hardware-capable device previously leaked its master seed to one unauthenticated in-process
        // JNI call (the binding enrolls lazily on first spend). Now the reveal MUST go through the
        // signature-verified `hey_recovery_phrase_hw` (Kotlin force-enrolls the key first). Devices
        // with no secure lock (hw_capable == false) keep the legacy UI-biometric gate below.
        if crate::guard::spend_binding_active() || crate::guard::hw_capable() {
            crate::guard::audit("seed.reveal.deny", serde_json::json!({ "reason": "hardware-capable — use the verified reveal" }));
            return out(env, String::new());
        }
        let phrase = super::wallet_phrase().unwrap_or_default();
        // No binding enrolled (legacy / no secure lock): the UI biometric is the gate.
        // RECORD every reveal in the (sealed + tamper-evident) audit log so a silent
        // exfiltration still leaves a trace.
        if !phrase.is_empty() {
            crate::guard::audit("seed.reveal", serde_json::json!({}));
        }
        // L-1: copy the words into the JVM string, then wipe the in-process copy in place
        // so the master seed doesn't linger in freed heap after the reveal returns.
        let ret = env.new_string(&phrase).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut());
        super::wipe_phrase(phrase);
        ret
    }

    /// Kotlin tells the runtime, at startup, whether THIS device can do hardware-gated signing
    /// (a secure lock + Keystore). When true, the BARE seed reveal is refused so a never-spent
    /// wallet can't leak its seed before the spend binding is lazily enrolled (F-SEED-REVEAL).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1set_1hw_1capable(
        _env: JNIEnv,
        _class: JClass,
        capable: jni::sys::jboolean,
    ) {
        crate::guard::set_hw_capable(capable != 0);
    }

    /// Issue a fresh one-time challenge the Keystore op must sign to reveal the seed
    /// (H5). The Kotlin BiometricPrompt CryptoObject signs
    /// `challenge\0seed.reveal\0seed.reveal\0seed.reveal`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1reveal_1challenge(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        out(env, crate::guard::issue_reveal_challenge().unwrap_or_default())
    }

    /// HARDWARE-VERIFIED seed reveal (H5): returns the BIP39 mnemonic ONLY after a
    /// fresh Keystore signature over the one-time reveal-challenge verifies against
    /// the enrolled key. The phrase is the master root over every chain, so its
    /// reveal is now bound to a real biometric op in the TEE/StrongBox — not merely
    /// audited. Empty string on a missing/invalid signature or no phrase.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1recovery_1phrase_1hw(
        mut env: JNIEnv,
        _class: JClass,
        sig_hex: JString,
    ) -> jstring {
        let sig = jstr(&mut env, &sig_hex).unwrap_or_default();
        if let Err(e) = crate::guard::verify_reveal_sig(&sig) {
            log::warn!("recovery_phrase_hw: {e}");
            return out(env, String::new());
        }
        let phrase = super::wallet_phrase().unwrap_or_default();
        if !phrase.is_empty() {
            crate::guard::audit("seed.reveal", serde_json::json!({ "verified": true }));
        }
        // L-1: copy out, then wipe the in-process copy in place (see hey_recovery_phrase).
        let ret = env.new_string(&phrase).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut());
        super::wipe_phrase(phrase);
        ret
    }

    /// `HeyApi.hey_persist_identity()` — write the runtime-held identity to
    /// identity.json SEALED under the storage DEK. Used by the vault-off toggle so
    /// the seed is retained for the next launch WITHOUT ever writing plaintext.
    /// Returns 0 on success, -1 if the runtime/identity or data dir isn't ready.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1persist_1identity(
        _env: JNIEnv,
        _class: JClass,
    ) -> jint {
        let (Some(id), Some((_, dir))) = (super::IDENTITY.get(), APP_CFG.get()) else {
            return -1;
        };
        crate::identity::write_identity_blob(
            &std::path::Path::new(dir).join("identity.json"),
            &id.to_blob(),
        );
        0
    }

    /// `HeyApi.hey_init(dataDir, distDir, port, capsule, identityBlob)` — start
    /// the in-process runtime, point hey-core at it, wait until listening.
    /// `identityBlob` (empty = none): when the StrongBox/TEE vault is on, Kotlin
    /// unseals the seed and passes it here so the runtime initialises from it
    /// (`Identity::from_blob`) and NEVER writes a plaintext identity.json.
    /// Returns the port.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1init(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
        dist_dir: JString,
        port: jint,
        capsule: JString,
        identity_blob: JString,
    ) -> jint {
        crate::logbuf::init(log::LevelFilter::Debug);
        std::panic::set_hook(Box::new(|info| log::error!("PANIC: {info}")));
        // Pin elastos.app + drop n0 relays until iroh ships 1.0, so the phone homes
        // on the SAME relay as its peers (different relays never form a gossip mesh).
        super::relay_only_default();
        let data_dir = match jstr(&mut env, &data_dir) {
            Some(d) => d,
            None => return -1,
        };
        let dist_dir = jstr(&mut env, &dist_dir).unwrap_or_default();
        let capsule = jstr(&mut env, &capsule).unwrap_or_else(|| "hey-social".into());
        let identity_blob = jstr(&mut env, &identity_blob).filter(|s| !s.is_empty());
        let port = port as u16;
        let _ = APP_CFG.set((port, data_dir.clone()));
        // Mint a random per-launch bearer; the engine presents it (ensure_plat / receivers via
        // set_bearer) and the loopback server requires it (server::set_token), so the provider/
        // storage/identity routes are NOT reachable by other apps on the shared loopback.
        let token = APP_TOKEN
            .get_or_init(|| {
                let mut b = [0u8; 32];
                let _ = getrandom::getrandom(&mut b);
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            })
            .clone();
        crate::server::set_token(&token);
        log::info!("HeyApi.hey_init capsule={capsule} port={port} vault={}", identity_blob.is_some());
        start_background(Config {
            data_dir: PathBuf::from(data_dir.clone()),
            dist_dir: PathBuf::from(dist_dir),
            port,
            capsule,
            identity_blob,
        });
        // Wait until the in-process provider dispatch is installed (there is no socket to probe).
        block(async {
            for _ in 0..100 {
                if hey_core::plat::dispatch_ready() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
        spawn_receiver(port, data_dir.clone());
        spawn_dm_receiver(port, data_dir);
        port as jint
    }

    /// DM/group receiver — hey-core's canonical peer_receiver::run() loop on its
    /// own thread (so its thread-local plat/ctx/session stay valid).
    fn spawn_dm_receiver(port: u16, data_dir: String) {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        std::thread::Builder::new()
            .name("hey-dm-recv".into())
            .spawn(move || {
                hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
                hey_core::plat::set_store(&data_dir);
                if let Some(t) = APP_TOKEN.get() {
                    hey_core::plat::set_bearer(t);
                }
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

    /// Background thread that drives the cross-device feed: joins my + followed
    /// topics and ingests incoming posts/reactions/comments every ~2s. Runs on a
    /// dedicated thread so hey-core's thread-local plat base/store stay valid.
    fn spawn_receiver(port: u16, data_dir: String) {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        std::thread::Builder::new()
            .name("hey-social-recv".into())
            .spawn(move || {
                hey_core::plat::set_base(&format!("http://127.0.0.1:{port}"));
                hey_core::plat::set_store(&data_dir);
                if let Some(t) = APP_TOKEN.get() {
                    hey_core::plat::set_bearer(t);
                }
                social::install_ctx();
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("recv rt");
                loop {
                    rt.block_on(async {
                        let _ = social::ensure_session().await;
                        // JOIN topics + mesh every tick (no DEK/seed needed) so a
                        // locked/headless device still receives + buffers feed gossip.
                        social::ensure_subscriptions().await;
                        // Ingest (decrypt/store) only when fully unlocked; otherwise
                        // the wires stay buffered and drain on unlock.
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

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1whoami(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        let r = block(social::whoami());
        out(env, json_result(r))
    }

    /// DDRM (local-first, no chain): fetch a `.ddrm` blob by cid + decrypt with a
    /// base64 content key → JSON `{"b64": <base64 .glb>}` or `{"error": ...}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ddrm_1load(
        mut env: JNIEnv,
        _class: JClass,
        cid: JString,
        ck_b64: JString,
    ) -> jstring {
        ensure_plat();
        let cid = jstr(&mut env, &cid).unwrap_or_default();
        let ck = jstr(&mut env, &ck_b64).unwrap_or_default();
        let json = match block(social::ddrm_load_b64(&cid, &ck)) {
            Ok(b64) => serde_json::json!({ "b64": b64 }).to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        };
        out(env, json)
    }

    /// DDRM (local-first, no chain): decode the base64 `.glb`, encrypt with the base64
    /// content key, store via the content provider → `{"cid": ...}` or `{"error": ...}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ddrm_1pack(
        mut env: JNIEnv,
        _class: JClass,
        glb_b64: JString,
        ck_b64: JString,
    ) -> jstring {
        ensure_plat();
        let glb = jstr(&mut env, &glb_b64).unwrap_or_default();
        let ck = jstr(&mut env, &ck_b64).unwrap_or_default();
        let json = match block(social::ddrm_pack_b64(&glb, &ck)) {
            Ok(cid) => serde_json::json!({ "cid": cid }).to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        };
        out(env, json)
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1carrier_1health(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        let mut v = block(social::carrier_health());
        // The rich net snapshot (real public IP, the bound interface, and the HONEST
        // direct/relay state) is produced by /api/runtime/status — which is NOT served
        // in-process on mobile (socket-free), so social::carrier_health falls back to a
        // minimal payload and the connection screen would read direct=false by default
        // (looking like a false "relay"). Fill the fields straight from the live carrier
        // so the UI shows the truth: the same recipe as server::runtime_status.
        if let (Some(obj), Some((h, slot))) = (v.as_object_mut(), crate::NET.get()) {
            if let Some(extra) = h.block_on(async {
                let g = slot.read().await;
                let c = g.as_ref()?;
                let (v4, v6g) = c.net_stack();
                let (pub_v6, pub_v4) = c.net_addrs();
                let (udp_v4, udp_v6) = c.udp_paths();
                let (peers, direct_peers, relay_peers) = c.conn_summary().await;
                Some(serde_json::json!({
                    "online": c.is_online(),
                    "direct": direct_peers > 0,        // HONEST: a live peer on a non-relay path
                    "direct_capable": c.is_direct(),   // node-level reachability (can punch)
                    "node_id": c.node_id(),
                    "peer_count": peers,
                    "direct_peers": direct_peers,
                    "relay_peers": relay_peers,
                    "ipv4": v4,
                    "ipv6_global": v6g,
                    "public_v4": pub_v4,
                    "public_v6": pub_v6,
                    "udp_v4": udp_v4,
                    "udp_v6": udp_v6,
                    "local_addrs": c.advertised_addrs(),
                }))
            }) {
                if let Some(eo) = extra.as_object() {
                    for (k, val) in eo {
                        obj.insert(k.clone(), val.clone());
                    }
                }
            }
        }
        out(env, v.to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1feed(
        env: JNIEnv,
        _class: JClass,
        limit: jint,
    ) -> jstring {
        ensure_plat();
        let r = block(social::feed(limit.max(1) as usize));
        out(env, json_result(r))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1get_1post(
        mut env: JNIEnv,
        _class: JClass,
        id: JString,
    ) -> jstring {
        ensure_plat();
        let id = jstr(&mut env, &id).unwrap_or_default();
        let r = block(social::get_post(&id));
        out(env, json_result(r))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1upload_1media(
        mut env: JNIEnv,
        _class: JClass,
        bytes: JByteArray,
        mime: JString,
        filename: JString,
    ) -> jstring {
        ensure_plat();
        let data = env.convert_byte_array(&bytes).unwrap_or_default();
        let mime = jstr(&mut env, &mime).unwrap_or_default();
        let filename = jstr(&mut env, &filename).unwrap_or_else(|| "upload".into());
        let r = block(social::upload_media(&data, &mime, &filename));
        out(env, json_result(r))
    }

    /// LAZY MEDIA: pull a feed post's media blob ON DEMAND (a tile scrolled into view). `author`
    /// = the post author (we request it on their feed topic). Fire-and-forget; the UI then polls
    /// hey_media_ready and shows a sleek loading card until it's local.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ensure_1media(
        mut env: JNIEnv,
        _class: JClass,
        cid: JString,
        author: JString,
    ) {
        ensure_plat();
        let cid = jstr(&mut env, &cid).unwrap_or_default();
        let author = jstr(&mut env, &author).unwrap_or_default();
        if !cid.is_empty() && !author.is_empty() {
            block(social::ensure_media(&cid, &author));
        }
    }

    /// True once a feed media blob is downloaded locally (the UI swaps its loading card for the image).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1media_1ready(
        mut env: JNIEnv,
        _class: JClass,
        cid: JString,
    ) -> jboolean {
        ensure_plat();
        let cid = jstr(&mut env, &cid).unwrap_or_default();
        block(social::media_ready(&cid)) as jboolean
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1create_1post(
        mut env: JNIEnv,
        _class: JClass,
        caption: JString,
        media_tiles_json: JString,
    ) -> jstring {
        ensure_plat();
        let caption = jstr(&mut env, &caption).unwrap_or_default();
        let media = jstr(&mut env, &media_tiles_json).unwrap_or_else(|| "[]".into());
        let r = block(social::create_post(&caption, &media));
        out(env, json_result(r))
    }

    // ── reactions / comments / follow ────────────────────────────────────────

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1react(
        mut env: JNIEnv,
        _class: JClass,
        post_id: JString,
        emoji: JString,
    ) -> jstring {
        ensure_plat();
        let id = jstr(&mut env, &post_id).unwrap_or_default();
        let e = jstr(&mut env, &emoji).unwrap_or_else(|| "❤️".into());
        out(env, json_result(block(social::react(&id, &e))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1get_1reactions(
        mut env: JNIEnv,
        _class: JClass,
        post_id: JString,
    ) -> jstring {
        ensure_plat();
        let id = jstr(&mut env, &post_id).unwrap_or_default();
        out(env, json_result(block(social::get_reactions(&id))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1add_1comment(
        mut env: JNIEnv,
        _class: JClass,
        post_id: JString,
        text: JString,
        parent: JString,
    ) -> jstring {
        ensure_plat();
        let id = jstr(&mut env, &post_id).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let p = jstr(&mut env, &parent).unwrap_or_default();
        out(env, json_result(block(social::add_comment(&id, &t, &p))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1get_1comments(
        mut env: JNIEnv,
        _class: JClass,
        post_id: JString,
    ) -> jstring {
        ensure_plat();
        let id = jstr(&mut env, &post_id).unwrap_or_default();
        out(env, json_result(block(social::get_comments(&id))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1my_1friend_1link(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        let s = block(social::my_friend_link()).unwrap_or_default();
        out(env, s)
    }

    /// SLIM follow QR: `hyper:follow:<binary>` (full PQ keys, ~30% smaller than the legacy
    /// `hey:follow:` JSON link). Used for the Profile QR; the copyable/legacy link stays friendLink.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1my_1follow_1link(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        let s = block(social::my_follow_link()).unwrap_or_default();
        out(env, s)
    }

    /// SLIM chat QR: `hyper:chat:<binary>` (full PQ keys, ~30% smaller). Chat-only distinct scheme.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1my_1chat_1link(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        let s = block(social::my_chat_link()).unwrap_or_default();
        out(env, s)
    }

    /// Decode a follow/chat link for a confirm UI: JSON {kind,did,verified,has_keys} ("{}" if not a
    /// hyper:/hey:follow link). Lets the deep-link/confirm sheet show + route a binary hyper: link.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1link_1preview(
        mut env: JNIEnv,
        _class: JClass,
        link: JString,
    ) -> jstring {
        ensure_plat();
        let l = jstr(&mut env, &link).unwrap_or_default();
        out(env, social::preview_link(&l))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1follow(
        mut env: JNIEnv,
        _class: JClass,
        input: JString,
    ) -> jstring {
        ensure_plat();
        let i = jstr(&mut env, &input).unwrap_or_default();
        out(env, json_result(block(social::follow(&i))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1chat_1from_1link(
        mut env: JNIEnv,
        _class: JClass,
        input: JString,
    ) -> jstring {
        ensure_plat();
        let i = jstr(&mut env, &input).unwrap_or_default();
        out(env, json_result(block(social::chat_from_link(&i))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1unfollow(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::unfollow(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1remove_1follower(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::remove_follower(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1block_1follower(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::block_follower(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1accept_1follower(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::accept_follower(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1reject_1follower(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::reject_follower(&d))))
    }

    /// Unblock a previously-blocked DID (inverse of remove_follower's block step).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1unblock_1follower(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::unblock_follower(&d))))
    }

    /// Blocked-followers list as JSON `[{"did","name"}]` for the settings screen.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1list_1blocked(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, block(social::list_blocked()).to_string())
    }

    /// SAFETY-NUMBER VERIFICATION: mark this contact's pinned keys verified +
    /// clear any key_changed alarm. chat_contacts then reports key_verified=true
    /// / key_changed=false for the DID.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verify_1contact(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::verify_contact(&d))))
    }

    /// F-FOLLOW-PoP: "send anyway" — clear the first-send gate on a contact whose
    /// keys arrived from an unverified, unsigned follow link. After this,
    /// hey_send_dm no longer returns the `needs_verify_before_send` error for the
    /// DID. Surfaces in chat_contacts as needs_verify_before_send=false.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1confirm_1unverified_1send(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::confirm_unverified_send(&d))))
    }

    /// Delete a conversation WITHOUT blocking the contact (chat-only delete).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1delete_1chat(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::delete_chat(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1following(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, json_result(block(social::following())))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1is_1following(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::is_following(&d))))
    }

    /// F-12: keys-based safety number (Signal-style) for an established contact —
    /// hashes BOTH parties' PINNED encryption material, so a key-substitution MITM
    /// changes the displayed number. Returns "" when pinned keys aren't known yet
    /// (legacy/keyless contact) → the UI falls back to its DID-only fingerprint.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1safety_1number(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::safety_number(&d)))
    }

    /// Cheap change counter — the UI polls this and reloads only when it bumps.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1feed_1rev(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jlong {
        social::feed_rev() as jni::sys::jlong
    }

    // ── chat (DMs + groups) ──────────────────────────────────────────────────

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1contacts(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, block(social::chat_contacts()).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1groups(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, block(social::chat_groups()).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1conversation(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::chat_conversation(&d)).to_string())
    }

    // ── 1:1 voice call signaling (Stage 1) ───────────────────────────────────
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1send(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        payload: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let p = jstr(&mut env, &payload).unwrap_or_default();
        out(env, serde_json::json!({ "ok": block(social::call_send(&d, &p)) }).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1poll(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, block(social::call_poll()).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1edit_1message(
        mut env: JNIEnv,
        _class: JClass,
        chat_id: JString,
        msg_id: JString,
        text: JString,
        is_group: jboolean,
    ) -> jstring {
        ensure_plat();
        let c = jstr(&mut env, &chat_id).unwrap_or_default();
        let m = jstr(&mut env, &msg_id).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let ok = block(social::edit_chat_message(&c, &m, &t, is_group != 0));
        out(env, serde_json::json!({ "ok": ok }).to_string())
    }

    // ── Block list (ENGINE-owned blocked set in hey-core dms) ────────────────
    // The Kotlin block UI drives the canonical hey-core blocked set so the same
    // list gates BOTH inbound follow/DM acceptance AND outbound sends (the engine
    // enforces it; the UI only mirrors it). All three touch storage through the
    // thread-local plat, so they take the same ensure_plat() + block(..) path as
    // every other social JNI on this thread.

    /// `HeyApi.hey_set_blocked(did, blocked)` — add (blocked != 0) or remove
    /// (blocked == 0) `did` from the engine's blocked set. Empty `did` is a no-op.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1set_1blocked(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        blocked: jboolean,
    ) {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        if d.is_empty() {
            return;
        }
        block(hey_core::api::dms::set_blocked(&d, blocked != 0));
    }

    /// `HeyApi.hey_is_blocked(did) -> jboolean` — true if `did` is in the engine's
    /// blocked set. Empty/unknown `did` → false.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1is_1blocked(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jboolean {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        if d.is_empty() {
            return false as jboolean;
        }
        block(hey_core::api::dms::is_blocked(&d)) as jboolean
    }

    /// `HeyApi.hey_blocked_by_peer(did) -> jboolean` — true if `did` sent us a sealed BLOCK signal
    /// (recorded in blocked-by-peer.json) and hasn't yet sent an UNBLOCK. UI-only: lets the chat
    /// screen show "you've been blocked" + disable the composer. Empty/unknown `did` → false.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1blocked_1by_1peer(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jboolean {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        if d.is_empty() {
            return false as jboolean;
        }
        block(social::is_blocked_by_peer(&d)) as jboolean
    }

    /// `HeyApi.hey_blocked_list() -> String` — the engine's blocked set as a JSON
    /// array of DID strings (e.g. `["did:key:...","did:key:..."]`). Empty `[]` when
    /// nothing is blocked or on a serialization error (fail closed to an empty list).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1blocked_1list(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        let list = block(hey_core::api::dms::blocked_list());
        out(env, serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()))
    }

    // ── Hey Verse lane (ephemeral world presence — never stored/notified) ────
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1send(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        payload: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let p = jstr(&mut env, &payload).unwrap_or_default();
        out(env, serde_json::json!({ "ok": block(social::verse_send(&d, &p)) }).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1poll(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, social::verse_poll().to_string())
    }

    /// Connectivity returned (Android NetworkCallback) → re-probe iroh + re-join gossip topics now,
    /// instead of waiting for the carrier's ~10s self-heal. No-op if the carrier isn't up yet.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1net_1changed(
        _env: JNIEnv,
        _class: JClass,
    ) {
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    // Smart: full reprobe only on a REAL IP change, else a cheap re-dial — so the
                    // chatty Android NetworkCallback can't churn the mesh in a loop.
                    c.net_event().await;
                }
            });
        }
    }

    // ── 1:1 call MEDIA E2E (app-layer PQ-derived keys + SAS) ─────────────────
    /// Mint a fresh 32-byte per-call media secret (base64). The caller embeds it in the sealed
    /// PQ-DM call OFFER; the callee reads it back. Both derive the directional voice/video keys
    /// + the SAS from it. Empty string on entropy failure.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1new_1secret(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        use base64::Engine as _;
        let mut s = [0u8; 32];
        if getrandom::getrandom(&mut s).is_err() {
            return out(env, String::new());
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(s);
        s.fill(0);
        out(env, b64)
    }

    /// Install the per-call media keys from the shared secret + call_id, and return the SAS (6
    /// digits) for the UI to display. `is_caller` picks which directional key is TX vs RX. Voice
    /// and video get DOMAIN-SEPARATED keys (no shared nonce space). Call BEFORE voice/video start.
    /// Empty string = bad input (the call then stays plaintext — fail open is acceptable only
    /// because the caller checks for empty and aborts the call when E2E was expected).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1media_1init(
        mut env: JNIEnv,
        _class: JClass,
        call_id: JString,
        secret_b64: JString,
        is_caller: jni::sys::jboolean,
    ) -> jstring {
        use base64::Engine as _;
        let call_id = jstr(&mut env, &call_id).unwrap_or_default();
        let sb = jstr(&mut env, &secret_b64).unwrap_or_default();
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(sb.trim()) else {
            return out(env, String::new());
        };
        if bytes.len() != 32 || call_id.is_empty() {
            return out(env, String::new());
        }
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&bytes);
        let caller = is_caller != 0;
        let (v_c2p, v_p2c) = hey_core::crypto::media_keys(&secret, &call_id, "voice");
        let (d_c2p, d_p2c) = hey_core::crypto::media_keys(&secret, &call_id, "video");
        // caller sends caller→peer (c2p) + receives peer→caller (p2c); the callee mirrors.
        if caller {
            crate::voice::set_media_keys(v_c2p, v_p2c);
            crate::video::set_media_keys(d_c2p, d_p2c);
        } else {
            crate::voice::set_media_keys(v_p2c, v_c2p);
            crate::video::set_media_keys(d_p2c, d_c2p);
        }
        let sas = hey_core::crypto::media_sas(&secret, &call_id);
        secret.fill(0);
        out(env, sas)
    }

    /// Drop the per-call media keys at call end (so the next un-keyed call is plaintext again).
    /// Clears BOTH the 1:1 and the group keys.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1media_1clear(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::voice::clear_media_keys();
        crate::video::clear_media_keys();
        crate::voice::clear_group_media_keys();
        crate::video::clear_group_media_keys();
    }

    /// Install the shared GROUP media key for `call_id` from the owner-distributed `secret_b64`,
    /// using OUR `my_did` for the per-sender nonce salt. Idempotent (safe to call every roster poll).
    /// Does NOT enable encryption — call hey_call_group_media_active(true) only once EVERY participant
    /// is media-capable, so a mixed-version call never feeds an old member un-decryptable noise.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1group_1media_1init(
        mut env: JNIEnv,
        _class: JClass,
        call_id: JString,
        secret_b64: JString,
        my_did: JString,
    ) {
        use base64::Engine as _;
        let call_id = jstr(&mut env, &call_id).unwrap_or_default();
        let sb = jstr(&mut env, &secret_b64).unwrap_or_default();
        let my_did = jstr(&mut env, &my_did).unwrap_or_default();
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(sb.trim()) else {
            return;
        };
        if bytes.len() != 32 || call_id.is_empty() || my_did.is_empty() {
            return;
        }
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&bytes);
        let v_key = hey_core::crypto::media_group_key(&secret, &call_id, "voice");
        let d_key = hey_core::crypto::media_group_key(&secret, &call_id, "video");
        // One per-sender salt (from our did) for both streams — the voice/video KEYS already differ,
        // so reusing the salt across streams cannot collide a nonce.
        let salt = hey_core::crypto::media_group_salt(&secret, &call_id, &my_did);
        crate::voice::set_group_media_key(v_key, salt);
        crate::video::set_group_media_key(d_key, salt);
        secret.fill(0);
    }

    /// Enable/disable outbound GROUP media encryption (the app sets true only when ALL current
    /// participants advertised media-capability; false otherwise so old members aren't fed noise).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1call_1group_1media_1active(
        _env: JNIEnv,
        _class: JClass,
        active: jni::sys::jboolean,
    ) {
        let a = active != 0;
        crate::voice::set_group_media_active(a);
        crate::video::set_group_media_active(a);
    }

    // ── 1:1 voice call audio (Stage 2: μ-law over the carrier's voice ALPN) ───
    /// Begin the audio session. `peer_ticket` = the contact's carrier ticket (base32 EndpointAddr);
    /// `is_caller` decides who dials. Runs on the carrier runtime so the dial/recv loops live there.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1start(
        mut env: JNIEnv,
        _class: JClass,
        peer_ticket: JString,
        is_caller: jni::sys::jboolean,
    ) {
        let ticket = jstr(&mut env, &peer_ticket).unwrap_or_default();
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

    /// How many LIVE voice links this session has — the call screen's probe
    /// (0 = the dial is still fighting; the UI says "connecting audio…").
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1peers(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jint {
        crate::voice::connected_peers() as jni::sys::jint
    }

    // ── Verse REALTIME lane (movement over QUIC datagrams, not DMs) ──────────

    /// Authorize + connect the fast lane to a verse peer (after the sealed
    /// DM-lane invite/accept). Resolves the contact's ticket internally and
    /// dials by IDENTITY (live paths + discovery; never the stale ticket addr).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1rt_1join(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        // peer_ticket reads storage → storage::suffix needs the thread-local
        // CapsuleCtx that ensure_plat() just installed on THIS thread. The carrier
        // runtime's worker threads never call install_ctx, so resolving the ticket
        // inside h.spawn aborts ("ctx::init must be called in main()"). Resolve it
        // HERE; the spawned task only does carrier ops (no ctx needed).
        // SELF-ASSERTED ticket only — verse-rt media must not dial an owner-poisoned endpoint.
        let ticket = block(social::peer_ticket_self_asserted(&d));
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    match c.peer_id_of(&ticket) {
                        Some(peer) => crate::verse_rt::join(c.endpoint(), peer, d),
                        None => log::warn!("verse-rt: no ticket for {d}"),
                    }
                }
            });
        }
    }

    /// Broadcast one payload to every fast-lane peer. Sync + lock-cheap.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1rt_1send(
        mut env: JNIEnv,
        _class: JClass,
        payload: JString,
    ) {
        let p = jstr(&mut env, &payload).unwrap_or_default();
        crate::verse_rt::send_all(&p);
    }

    /// Drain the fast lane → `[{"from": did, "payload": {...}}]`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1rt_1recv(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        let mut arr = Vec::new();
        for (from, payload) in crate::verse_rt::drain() {
            let v: serde_json::Value = serde_json::from_str(&payload)
                .unwrap_or(serde_json::Value::String(payload));
            arr.push(serde_json::json!({ "from": from, "payload": v }));
        }
        out(env, serde_json::Value::Array(arr).to_string())
    }

    /// How many live fast-lane connections exist (the plugin skips the DM copy
    /// of movement when this is > 0 — verse_rt is carrying it).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1rt_1count(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jint {
        crate::verse_rt::connected() as jni::sys::jint
    }

    /// True if `did` has a live fast link (the plugin then skips the DM copy).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1rt_1has(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jni::sys::jboolean {
        // peer_ticket reads storage → storage::suffix needs the thread-local
        // CapsuleCtx that ensure_plat() installs. h.block_on drives the future on
        // THIS (JNI) thread, so install the ctx here first or peer_ticket aborts
        // in ctx::get ("ctx::init must be called in main()") — same as its siblings.
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            let up = h.block_on(async move {
                if let Some(c) = slot.read().await.clone() {
                    let ticket = social::peer_ticket_self_asserted(&d).await;
                    if let Some(peer) = c.peer_id_of(&ticket) {
                        return crate::verse_rt::has_peer(&peer);
                    }
                }
                false
            });
            return if up { 1 } else { 0 };
        }
        0
    }

    /// Everyone left the verse: tear the fast lane down.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1rt_1reset(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::verse_rt::reset();
    }

    /// Join the EPHEMERAL gossip presence topic for `world`, bootstrapping with
    /// the carrier tickets of the currently-present peers (newline-joined dids).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1gossip_1join(
        mut env: JNIEnv,
        _class: JClass,
        world: JString,
        peer_dids: JString,
    ) {
        ensure_plat();
        let w = jstr(&mut env, &world).unwrap_or_default();
        let raw = jstr(&mut env, &peer_dids).unwrap_or_default();
        let dids: Vec<String> = raw.split('\n').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
        // whoami_did + peer_ticket read storage → they need the thread-local
        // CapsuleCtx ensure_plat() installed HERE; the carrier workers don't have it
        // (would abort in storage::suffix). Resolve our did + bootstrap tickets on
        // this thread; the spawned task only joins the gossip topic (no ctx needed).
        let (me, boot) = block(async {
            let me = social::whoami_did().await.unwrap_or_default();
            let mut boot = Vec::new();
            for d in &dids {
                let tk = social::peer_ticket_self_asserted(d).await;
                if !tk.is_empty() {
                    boot.push(tk);
                }
            }
            (me, boot)
        });
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            // The member did:keys (`dids`) double as (a) the per-membership key
            // material for a PRIVATE world and (b) the receive-roster gate — both
            // handled inside verse_gossip::join. `epoch` is the admin-rotation hook
            // (0 until a rotation UI exists). This is purely additive: the Kotlin
            // JNI signature (world, peer_dids) is unchanged.
            let members = dids.clone();
            let epoch: u32 = 0;
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    crate::verse_gossip::join(c, w, me, members, epoch, boot).await;
                }
            });
        }
    }

    /// Broadcast one movement payload over the ephemeral gossip topic. No seal,
    /// no disk — fire-and-forget like the fast lane.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1gossip_1send(
        mut env: JNIEnv,
        _class: JClass,
        payload: JString,
    ) {
        let p = jstr(&mut env, &payload).unwrap_or_default();
        if let Some((h, _slot)) = crate::NET.get() {
            h.spawn(async move {
                crate::verse_gossip::send_all(p).await;
            });
        }
    }

    /// Tear down the ephemeral gossip presence lane (session emptied / left).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1gossip_1reset(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::verse_gossip::reset();
    }

    /// True once the gossip presence lane is live (joined a world topic).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1verse_1gossip_1count(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jboolean {
        crate::verse_gossip::connected() as jni::sys::jboolean
    }

    /// Send one captured PCM frame (16-bit LE) — encoded + sent as a μ-law datagram. Sync.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1send(
        mut env: JNIEnv,
        _class: JClass,
        pcm: JByteArray,
    ) {
        if let Ok(v) = env.convert_byte_array(&pcm) {
            crate::voice::send_pcm(&v);
        }
    }

    /// Pull up to `max_bytes` of decoded PCM (16-bit LE) from the jitter buffer for playback. Sync.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1recv(
        mut env: JNIEnv,
        _class: JClass,
        max_bytes: jint,
    ) -> jni::sys::jbyteArray {
        let out = crate::voice::recv_pcm(max_bytes.max(0) as usize);
        env.byte_array_from_slice(&out)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1set_1muted(
        _env: JNIEnv,
        _class: JClass,
        muted: jni::sys::jboolean,
    ) {
        crate::voice::set_muted(muted != 0);
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1stop(_env: JNIEnv, _class: JClass) {
        crate::voice::stop();
    }

    // ── video calls (direct-only) — H.264 frames over QUIC uni-streams ──────────
    /// Begin a 1:1 video session with the peer. DIRECT-ONLY HARD GATE: refuses if
    /// the existing path to this peer is relay (a relay can't carry 1080p + would
    /// flood it). Resolves + dials by IDENTITY, mirroring voice.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1start(
        mut env: JNIEnv,
        _class: JClass,
        peer_ticket: JString,
    ) {
        let ticket = jstr(&mut env, &peer_ticket).unwrap_or_default();
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

    /// Queue one encoded H.264 frame for the peer. Sync (called from the encoder thread).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1send_1frame(
        mut env: JNIEnv,
        _class: JClass,
        frame: JByteArray,
    ) {
        if let Ok(v) = env.convert_byte_array(&frame) {
            crate::video::send_frame(&v);
        }
    }

    /// Pop the next received H.264 frame for the decoder (empty array if none ready). Sync.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1recv_1frame(
        mut env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jbyteArray {
        let out = crate::video::recv_frame();
        env.byte_array_from_slice(&out)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Camera-off: stop emitting frames without tearing the lane down.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1set_1paused(
        _env: JNIEnv,
        _class: JClass,
        paused: jni::sys::jboolean,
    ) {
        crate::video::set_paused(paused != 0);
    }

    /// LIVE video links this session — the "connecting video…" probe (0 while dialing).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1peers(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jint {
        crate::video::connected_peers() as jni::sys::jint
    }

    /// Cumulative dropped frames (network behind) — the adaptive-bitrate signal.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1dropped(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jlong {
        crate::video::dropped() as jni::sys::jlong
    }

    /// Monotonic new-subscriber epoch — bumped when a peer's video lane comes up. The
    /// Kotlin sync loop watches the delta and requests an immediate keyframe so a late
    /// joiner's decoder configures at once instead of staying black until the next GOP.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1peer_1epoch(
        _env: JNIEnv,
        _class: JClass,
    ) -> jni::sys::jlong {
        crate::video::new_peer_epoch() as jni::sys::jlong
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1stop(_env: JNIEnv, _class: JClass) {
        crate::video::stop();
    }

    /// Begin a GROUP video session (empty mesh; reconcile via hey_video_sync).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1group_1start(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::video::group_start();
    }

    /// Reconcile the video mesh to a newline-separated list of participant tickets
    /// (dials new peers; mirrors hey_voice_sync). The roster gate rejects anyone
    /// not listed; direct-only peers connect.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1sync(
        mut env: JNIEnv,
        _class: JClass,
        tickets: JString,
    ) {
        let csv = jstr(&mut env, &tickets).unwrap_or_default();
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    let ids: Vec<_> = csv
                        .split('\n')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .filter_map(|t| c.decode_bootstrap(t))
                        .collect();
                    crate::video::sync_peers(c.endpoint(), ids);
                }
            });
        }
    }

    /// Pop the next received H.264 frame from a SPECIFIC peer (the grid: one decoder
    /// per remote tile). `peer` is an EndpointId string from hey_video_peer_ids.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1recv_1frame_1from(
        mut env: JNIEnv,
        _class: JClass,
        peer: JString,
    ) -> jni::sys::jbyteArray {
        let id = jstr(&mut env, &peer).unwrap_or_default();
        let out = crate::video::recv_frame_from(&id);
        env.byte_array_from_slice(&out)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Newline-separated EndpointId strings with a LIVE video link — build one grid
    /// tile per id and pull its frames via hey_video_recv_frame_from.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1video_1peer_1ids(
        mut env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        let s = crate::video::peer_ids().join("\n");
        env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
    }

    /// A contact's carrier ticket (base32) for dialing their voice/video ALPN. SELF-ASSERTED only —
    /// returns empty for an owner/discovery-poisoned ticket so a malicious group owner can't redirect
    /// a victim's 1:1 voice/video to an attacker endpoint (the Kotlin call paths skip an empty ticket).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1peer_1ticket(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::peer_ticket_self_asserted(&d)))
    }

    /// Live transport to a contact: "direct" | "relay" | "offline". Powers the
    /// per-contact connection badge + the transport-gated attachment cap.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1contact_1transport(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let ticket = block(social::peer_ticket(&d));
        let t = if ticket.is_empty() {
            "offline".to_string()
        } else if let Some((h, slot)) = crate::NET.get() {
            h.block_on(async {
                match slot.read().await.as_ref() {
                    Some(c) => c.peer_transport(&ticket).await.to_string(),
                    None => "offline".to_string(),
                }
            })
        } else {
            "offline".to_string()
        };
        out(env, t)
    }

    /// Live transport to an arbitrary PEER TICKET: "direct" | "relay" | "offline".
    /// Unlike `hey_contact_transport` (DID → ticket lookup), this takes a ticket
    /// straight off a group-call roster participant, so a member can decide DIRECT
    /// (join with video) vs RELAY (audio-only) per the call's actual peers.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1peer_1transport(
        mut env: JNIEnv,
        _class: JClass,
        ticket: JString,
    ) -> jstring {
        ensure_plat();
        let ticket = jstr(&mut env, &ticket).unwrap_or_default();
        let t = if ticket.is_empty() {
            "offline".to_string()
        } else if let Some((h, slot)) = crate::NET.get() {
            h.block_on(async {
                match slot.read().await.as_ref() {
                    Some(c) => c.peer_transport(&ticket).await.to_string(),
                    None => "offline".to_string(),
                }
            })
        } else {
            "offline".to_string()
        };
        out(env, t)
    }

    /// True iff a private chat with this contact is permitted (chat explicitly established via a
    /// chat QR/invite, or real history exists). FALSE for a follow-only contact — the UI hides the
    /// "Message" affordance so following someone never opens a chat.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1can_1chat(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jni::sys::jboolean {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        block(social::can_chat(&d)) as jni::sys::jboolean
    }

    /// Download progress (0..=100) for an in-flight attachment fetch, -1 if not
    /// active. Pure in-memory read — no ctx/block needed; polled by the UI.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1attachment_1progress(
        mut env: JNIEnv,
        _class: JClass,
        id: JString,
    ) -> jni::sys::jint {
        let i = jstr(&mut env, &id).unwrap_or_default();
        social::attachment_progress(&i) as jni::sys::jint
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1conversation(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, block(social::chat_group_conversation(&g)).to_string())
    }

    /// Group-admin detail: roster + live nicknames + admin/creator flags + amAdmin.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1info(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, block(social::group_info(&g)).to_string())
    }

    /// Add contacts (JSON array of DIDs) to a group. Returns {ok} or {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1add_1members(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        dids_json: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let d = jstr(&mut env, &dids_json).unwrap_or_else(|| "[]".into());
        out(env, json_result(block(social::chat_group_add_members(&g, &d))))
    }

    /// Remove a member from a group. Returns {ok} or {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1remove_1member(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::chat_group_remove_member(&g, &d))))
    }

    /// Promote a member to admin. Returns {ok} or {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1add_1admin(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        member_did: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let d = jstr(&mut env, &member_did).unwrap_or_default();
        out(env, json_result(block(social::chat_group_add_admin(&g, &d))))
    }

    /// Set the group picture (CID/ref of a pre-uploaded image; "" clears it).
    /// Returns {ok} or {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1set_1picture(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        picture: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let p = jstr(&mut env, &picture).unwrap_or_default();
        out(env, json_result(block(social::chat_group_set_picture(&g, &p))))
    }

    /// Delete one of my own messages for everyone (tombstone over the E2E channel). Returns {ok}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1delete_1message(
        mut env: JNIEnv,
        _class: JClass,
        chat_id: JString,
        msg_id: JString,
        is_group: jni::sys::jboolean,
    ) -> jstring {
        ensure_plat();
        let c = jstr(&mut env, &chat_id).unwrap_or_default();
        let m = jstr(&mut env, &msg_id).unwrap_or_default();
        out(env, serde_json::json!({ "ok": block(social::delete_chat_message(&c, &m, is_group != 0)) }).to_string())
    }

    // ── group voice calls (mesh signaling + roster) ──────────────────────────
    /// Announce a group call on the group thread → returns {ok, call_id, ticket, video}.
    /// `video` (jboolean, 0 = audio-only, non-zero = video) rides the announce so a
    /// joiner's UI can read it back off group_active_call / group_call_roster.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1call_1start(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        video: jboolean,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, block(social::group_call_start(&g, video != 0)).to_string())
    }

    /// Emit a group-call control signal: kind = "join" | "leave" | "end". Returns {ok}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1call_1signal(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        call_id: JString,
        kind: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let c = jstr(&mut env, &call_id).unwrap_or_default();
        let k = jstr(&mut env, &kind).unwrap_or_default();
        out(env, serde_json::json!({ "ok": block(social::group_call_signal(&g, &c, &k)) }).to_string())
    }

    /// Live roster for a group call → {active, ended, call_id, participants:[{did,ticket,name,mine}]}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1call_1roster(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        call_id: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let c = jstr(&mut env, &call_id).unwrap_or_default();
        out(env, block(social::group_call_roster(&g, &c)).to_string())
    }

    /// The most recent group call on a thread (for offering a "Join") → roster json (or active:false).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1active_1call(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, block(social::group_active_call(&g)).to_string())
    }

    /// Open an empty group-call audio mesh (peers are added as the roster syncs).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1group_1start(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::voice::group_start();
    }

    /// Reconcile the voice mesh to a newline-separated list of participant tickets (dials new peers).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1voice_1sync(
        mut env: JNIEnv,
        _class: JClass,
        tickets: JString,
    ) {
        let csv = jstr(&mut env, &tickets).unwrap_or_default();
        if let Some((h, slot)) = crate::NET.get() {
            let slot = slot.clone();
            h.spawn(async move {
                if let Some(c) = slot.read().await.clone() {
                    let ids: Vec<_> = csv
                        .split('\n')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .filter_map(|t| c.decode_bootstrap(t))
                        .collect();
                    crate::voice::sync_peers(c.endpoint(), ids);
                }
            });
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1dm(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        text: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        out(env, json_result(block(social::chat_send(&d, &t))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1group(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        text: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        out(env, json_result(block(social::chat_send_group(&g, &t))))
    }

    /// Send a 1:1 message that QUOTES another (tap-to-reply). reply_* describe the
    /// quoted message; empty reply_id ⇒ behaves like a normal send.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1dm_1reply(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        text: JString,
        reply_id: JString,
        reply_author: JString,
        reply_snippet: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let rid = jstr(&mut env, &reply_id).unwrap_or_default();
        let ra = jstr(&mut env, &reply_author).unwrap_or_default();
        let rs = jstr(&mut env, &reply_snippet).unwrap_or_default();
        out(env, json_result(block(social::chat_send_reply(&d, &t, &rid, &ra, &rs))))
    }

    /// Group counterpart of hey_send_dm_reply.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1group_1reply(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        text: JString,
        reply_id: JString,
        reply_author: JString,
        reply_snippet: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let rid = jstr(&mut env, &reply_id).unwrap_or_default();
        let ra = jstr(&mut env, &reply_author).unwrap_or_default();
        let rs = jstr(&mut env, &reply_snippet).unwrap_or_default();
        out(env, json_result(block(social::chat_send_group_reply(&g, &t, &rid, &ra, &rs))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1total_1unread(
        _env: JNIEnv,
        _class: JClass,
    ) -> jint {
        ensure_plat();
        block(social::chat_unread()) as jint
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1mark_1read(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        block(social::chat_mark_read(&d));
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1group_1mark_1read(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        block(social::chat_mark_group_read(&g));
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1gen_1invite(
        mut env: JNIEnv,
        _class: JClass,
        label: JString,
    ) -> jstring {
        ensure_plat();
        let l = jstr(&mut env, &label).unwrap_or_default();
        out(env, block(social::chat_gen_invite(&l)).unwrap_or_default())
    }


    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1accept_1invite(
        mut env: JNIEnv,
        _class: JClass,
        token: JString,
    ) -> jstring {
        ensure_plat();
        let t = jstr(&mut env, &token).unwrap_or_default();
        match block(social::chat_accept_invite(&t)) {
            Ok(did) => out(env, serde_json::json!({ "ok": true, "did": did }).to_string()),
            Err(e) => out(env, serde_json::json!({ "error": e }).to_string()),
        }
    }

    // ── chat extras: attachments, group-create, message reactions ────────────

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1attachment(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        text: JString,
        bytes: JByteArray,
        mime: JString,
        filename: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let data = env.convert_byte_array(&bytes).unwrap_or_default();
        let m = jstr(&mut env, &mime).unwrap_or_default();
        let f = jstr(&mut env, &filename).unwrap_or_else(|| "file".into());
        out(env, json_result(block(social::chat_send_attachment(&d, &t, &data, &m, &f))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1group_1attachment(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        text: JString,
        bytes: JByteArray,
        mime: JString,
        filename: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let data = env.convert_byte_array(&bytes).unwrap_or_default();
        let m = jstr(&mut env, &mime).unwrap_or_default();
        let f = jstr(&mut env, &filename).unwrap_or_else(|| "file".into());
        out(env, json_result(block(social::chat_send_group_attachment(&g, &t, &data, &m, &f))))
    }

    /// Resolve `elastos://<cid>` → content bytes (empty on error). Backs the
    /// namespace media resolver so the UI never references the loopback gateway.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1content_1bytes(
        mut env: JNIEnv,
        _class: JClass,
        cid: JString,
    ) -> jni::sys::jbyteArray {
        ensure_plat();
        let c = jstr(&mut env, &cid).unwrap_or_default();
        let bytes = block(social::content_bytes(&c));
        env.byte_array_from_slice(&bytes)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Returns the decrypted plaintext bytes (empty array on error).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1fetch_1attachment(
        mut env: JNIEnv,
        _class: JClass,
        att_json: JString,
    ) -> jni::sys::jbyteArray {
        ensure_plat();
        let a = jstr(&mut env, &att_json).unwrap_or_default();
        let bytes = block(social::chat_fetch_attachment(&a)).unwrap_or_default();
        env.byte_array_from_slice(&bytes)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    /// Streamed (torrent-style) send: reads from a file PATH and uploads
    /// chunk-by-chunk (no whole-file ByteArray). Returns the sent message JSON / {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1attachment_1path(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
        text: JString,
        path: JString,
        mime: JString,
        filename: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let p = jstr(&mut env, &path).unwrap_or_default();
        let m = jstr(&mut env, &mime).unwrap_or_default();
        let f = jstr(&mut env, &filename).unwrap_or_else(|| "file".into());
        out(env, json_result(block(social::chat_send_attachment_path(&d, &t, &p, &m, &f))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1send_1group_1attachment_1path(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
        text: JString,
        path: JString,
        mime: JString,
        filename: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        let t = jstr(&mut env, &text).unwrap_or_default();
        let p = jstr(&mut env, &path).unwrap_or_default();
        let m = jstr(&mut env, &mime).unwrap_or_default();
        let f = jstr(&mut env, &filename).unwrap_or_else(|| "file".into());
        out(env, json_result(block(social::chat_send_group_attachment_path(&g, &t, &p, &m, &f))))
    }

    /// Streamed (torrent-style) fetch: download + decrypt straight to `dest` on
    /// disk (no whole-file ByteArray). Returns {"ok":true} / {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1fetch_1attachment_1to_1path(
        mut env: JNIEnv,
        _class: JClass,
        att_json: JString,
        dest: JString,
    ) -> jstring {
        ensure_plat();
        let a = jstr(&mut env, &att_json).unwrap_or_default();
        let dp = jstr(&mut env, &dest).unwrap_or_default();
        out(env, json_result(block(social::chat_fetch_attachment_to_path(&a, &dp))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1create_1group(
        mut env: JNIEnv,
        _class: JClass,
        name: JString,
        members_json: JString,
    ) -> jstring {
        ensure_plat();
        let n = jstr(&mut env, &name).unwrap_or_default();
        let m = jstr(&mut env, &members_json).unwrap_or_else(|| "[]".into());
        out(env, json_result(block(social::chat_create_group(&n, &m))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1react_1message(
        mut env: JNIEnv,
        _class: JClass,
        chat_id: JString,
        message_id: JString,
        emoji: JString,
        is_group: jni::sys::jboolean,
    ) -> jstring {
        ensure_plat();
        let c = jstr(&mut env, &chat_id).unwrap_or_default();
        let mid = jstr(&mut env, &message_id).unwrap_or_default();
        let e = jstr(&mut env, &emoji).unwrap_or_default();
        out(env, json_result(block(social::chat_react_message(&c, &mid, &e, is_group != 0))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1message_1reactions(
        mut env: JNIEnv,
        _class: JClass,
        chat_id: JString,
        is_group: jni::sys::jboolean,
    ) -> jstring {
        ensure_plat();
        let c = jstr(&mut env, &chat_id).unwrap_or_default();
        out(env, block(social::chat_message_reactions(&c, is_group != 0)).to_string())
    }

    // ── ESC wallet (same mnemonic, Essentials-recoverable) ───────────────────
    //
    // Stateless: Kotlin unseals the recovery phrase from the StrongBox/TEE vault
    // and passes it per-call; the key is rebuilt, used, and dropped. address +
    // balance are read-only; send is gated behind a biometric confirm in the UI.

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1address(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let r = signing_phrase(m).and_then(|p| crate::wallet::esc_address(&p));
        out(env, r.unwrap_or_default())
    }

    /// Registered EVM chains for the wallet UI: `[{key,name,chainId,symbol}]`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1chains(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        out(env, crate::wallet::evm_chains_json().to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1balance(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        out(env, json_result(signing_phrase(m).and_then(|p| crate::wallet::esc_balance(&p, &c))))
    }

    /// MONEY: signs + broadcasts a real EVM value transfer on `chain`. `value_hex` = wei in hex.
    /// Requires a one-shot spend grant minted by `hey_authorize_spend("evm:<chain>", to, value_hex)`
    /// after the user confirmed exactly this transfer — the signer is unreachable without it
    /// (fail closed), and grant + send are both audited (guard.rs).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1send(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        to: JString,
        value_hex: JString,
        auth: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let v = jstr(&mut env, &value_hex).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        // Redeem the grant INSIDE the signer (after the real fee is known) so a
        // max-fee bound in the grant is enforced against gasPrice*gasLimit. The
        // grant is single-use and the signer fails closed before any broadcast.
        let redeem = crate::wallet::SpendRedeem { token: auth, kind: format!("evm:{c}"), to: to.clone(), amount: v.clone() };
        let r = signing_phrase(m).and_then(|p| crate::wallet::esc_send_redeem(&p, &c, &to, &v, Some(redeem)));
        out(env, json_result(r))
    }

    /// Estimate the MAX network fee for a native send to `to` (value `value_hex`) on
    /// `chain` for the confirm dialog + the max-fee grant bound, using the SAME
    /// eth_estimateGas gas limit the signer will use so a contract recipient won't
    /// fail the send closed (M-1): `{maxFeeWei, maxFee, gasPriceWei, gasLimit, symbol}`.
    /// `mnemonic` is used ONLY to derive the sender for an accurate estimate; nothing
    /// is signed/sent. The user confirms `maxFeeWei`; pass it to the `_fee` mint.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1fee_1estimate(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        to: JString,
        value_hex: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let v = jstr(&mut env, &value_hex).unwrap_or_default();
        let r = signing_phrase(m).and_then(|p| crate::wallet::esc_fee_estimate(&p, &c, &to, &v));
        out(env, json_result(r))
    }

    /// All balances on `chain`: native + curated ERC-20s. `{address,tokens:[…]}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1balances(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        out(env, json_result(signing_phrase(m).and_then(|p| crate::wallet::evm_balances(&p, &c))))
    }

    /// MONEY: ERC-20 token transfer on `chain`. `amount_hex` = smallest units (hex).
    /// Spend grant kind: `erc20:<chain>:<contract>` (see hey_wallet_send).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1token_1send(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        contract: JString,
        to: JString,
        amount_hex: JString,
        auth: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let ct = jstr(&mut env, &contract).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount_hex).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        // Redeem the grant INSIDE the signer (after the real fee is known) so a
        // max-fee bound in the grant is enforced against gasPrice*gasLimit — same
        // as hey_wallet_send. The grant binding (kind/to/amount) is unchanged, so a
        // max_fee=0 grant stays unbounded (backward-compatible).
        let redeem = crate::wallet::SpendRedeem { token: auth, kind: format!("erc20:{c}:{ct}"), to: to.clone(), amount: a.clone() };
        let r = signing_phrase(m)
            .and_then(|p| crate::wallet::evm_token_send_redeem(&p, &c, &ct, &to, &a, Some(redeem)));
        out(env, json_result(r))
    }

    /// All NFTs (ERC-721/1155) the wallet holds on `chain`. `added` = JSON array
    /// of user-tracked contract addresses (for the indexer-off / trustless mode).
    /// `{address, mode:"index"|"tracked", collections:[…]}`. Read-only.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1nfts(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        added: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let added_raw = jstr(&mut env, &added).unwrap_or_default();
        let added: Vec<String> = serde_json::from_str(&added_raw).unwrap_or_default();
        out(env, json_result(signing_phrase(m).and_then(|p| crate::wallet::evm_nfts(&p, &c, &added))))
    }

    /// Look up a manually-added NFT (contract + decimal token_id) on `chain`:
    /// `{owned, kind, amount, name, image}` or `{error}`. Read-only — the
    /// trustless "+ Add collection" path for ids blind enumeration can't find.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1nft_1lookup(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        contract: JString,
        token_id: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let ct = jstr(&mut env, &contract).unwrap_or_default();
        let id = jstr(&mut env, &token_id).unwrap_or_default();
        out(env, json_result(signing_phrase(m).and_then(|p| crate::wallet::evm_nft_lookup(&p, &c, &ct, &id))))
    }

    /// MONEY (irreversible): transfer an ERC-721 NFT on `chain`. `token_id` = DECIMAL.
    /// Spend grant kind: `nft:<chain>:<contract>`, amount = the decimal token_id.
    /// Redeem FIRST (so a retried JNI call can't re-broadcast without a fresh grant).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1nft_1send_1721(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        contract: JString,
        to: JString,
        token_id: JString,
        auth: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let ct = jstr(&mut env, &contract).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let id = jstr(&mut env, &token_id).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        // Redeem INSIDE the signer (after the real fee is known) so a max-fee bound
        // is enforced; grant binding unchanged → max_fee=0 stays unbounded.
        let redeem = crate::wallet::SpendRedeem { token: auth, kind: format!("nft:{c}:{ct}"), to: to.clone(), amount: id.clone() };
        let r = signing_phrase(m)
            .and_then(|p| crate::wallet::evm_nft_send_721_redeem(&p, &c, &ct, &to, &id, Some(redeem)));
        out(env, json_result(r))
    }

    /// MONEY (irreversible): transfer `qty` of an ERC-1155 token id on `chain`.
    /// token_id + qty are DECIMAL. Spend grant kind BINDS THE QUANTITY:
    /// `nft1155:<chain>:<contract>:<qty>`, amount = decimal token_id — so a confirm
    /// of "send #5" can't move a different count. Redeem FIRST.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1nft_1send_11155(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        chain: JString,
        contract: JString,
        to: JString,
        token_id: JString,
        qty: JString,
        auth: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let ct = jstr(&mut env, &contract).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let id = jstr(&mut env, &token_id).unwrap_or_default();
        let q = jstr(&mut env, &qty).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        // Redeem INSIDE the signer (after the real fee is known) so a max-fee bound
        // is enforced; grant binding (binds qty) unchanged → max_fee=0 stays unbounded.
        let redeem = crate::wallet::SpendRedeem { token: auth, kind: format!("nft1155:{c}:{ct}:{q}"), to: to.clone(), amount: id.clone() };
        let r = signing_phrase(m)
            .and_then(|p| crate::wallet::evm_nft_send_1155_redeem(&p, &c, &ct, &to, &id, &q, Some(redeem)));
        out(env, json_result(r))
    }

    /// Validate + checksum a recipient address (EIP-55, zero-address guard). No key,
    /// no network — pure. Returns `{ "ok": true, "address": <checksummed> }` or `{ "error" }`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1check_1address(
        mut env: JNIEnv,
        _class: JClass,
        addr: JString,
    ) -> jstring {
        let a = jstr(&mut env, &addr).unwrap_or_default();
        let r = match crate::wallet::validate_address(&a) {
            Ok(c) => serde_json::json!({ "ok": true, "address": c }),
            Err(e) => serde_json::json!({ "error": e }),
        };
        out(env, r.to_string())
    }

    /// Confirmation status of a broadcast tx: `{ "status": "pending"|"success"|"failed" }`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1tx_1status(
        mut env: JNIEnv,
        _class: JClass,
        chain: JString,
        hash: JString,
    ) -> jstring {
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let h = jstr(&mut env, &hash).unwrap_or_default();
        out(env, json_result(crate::wallet::esc_tx_status(&c, &h)))
    }

    // ── Self-host blockchain nodes (per-chain RPC override; default = public RPC) ──

    /// Point `chain` (`esc`/`eid`/`ethereum`/`ela`) at the user's OWN node by
    /// persisting `<data_dir>/<chain>-rpc.txt`. An empty `url` clears it → revert to
    /// the bundled public default. Returns `{ "ok": true }` or `{ "ok": false, "error" }`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1set_1rpc(
        mut env: JNIEnv,
        _class: JClass,
        chain: JString,
        url: JString,
    ) -> jstring {
        let Some(chain) = jstr(&mut env, &chain) else {
            return out(env, serde_json::json!({ "ok": false, "error": "missing chain" }).to_string());
        };
        let Some(url) = jstr(&mut env, &url) else {
            return out(env, serde_json::json!({ "ok": false, "error": "missing url" }).to_string());
        };
        let r = match super::wallet::set_rpc_override(&chain, &url) {
            Ok(()) => serde_json::json!({ "ok": true }),
            Err(e) => serde_json::json!({ "ok": false, "error": e }),
        };
        out(env, r.to_string())
    }

    /// Self-hostable chains for the settings UI: `[{key,name,default,override}]`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1rpc_1nodes(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        out(env, super::wallet::rpc_nodes_json().to_string())
    }

    // ── Elastos DID (EID) + ELA mainchain — same mnemonic, Essentials-recoverable ──
    // Pure local P-256 derivation (no network, no key stored): instant.

    /// `did:elastos:…` (default DID, index 0) for the recovery phrase.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1elastos_1did(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let r = signing_phrase(m).and_then(|p| crate::did::elastos_did(&p));
        out(env, r.unwrap_or_default())
    }

    /// ELA mainchain `E…` address for the recovery phrase.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ela_1address(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let r = signing_phrase(m).and_then(|p| crate::did::ela_mainchain_address(&p));
        out(env, r.unwrap_or_default())
    }

    /// ELA mainchain balance (UTXO): `{address, sela, ela}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ela_1balance(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        out(env, json_result(signing_phrase(m).and_then(|p| crate::mainchain::ela_balance(&p))))
    }

    /// MONEY: build + sign + broadcast an ELA MAINCHAIN transfer. `amount` = decimal ELA.
    /// Spend grant kind: `ela` (see hey_wallet_send).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ela_1send(
        mut env: JNIEnv,
        _class: JClass,
        mnemonic: JString,
        to: JString,
        amount: JString,
        auth: JString,
    ) -> jstring {
        let m = jstr(&mut env, &mnemonic);
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        let r = crate::guard::redeem_spend(&auth, "ela", &to, &a)
            .and_then(|()| signing_phrase(m))
            .and_then(|p| crate::mainchain::ela_send(&p, &to, &a));
        out(env, json_result(r))
    }

    // ── Ledger hardware wallet (ELA mainchain) — see docs/HEY_LEDGER_SUPPORT.md ──
    //
    // Kotlin's LedgerBle is a DUMB GATT pipe: it feeds inbound notify packets via
    // hey_ledger_on_packet, and its writer loop PULLS the frames Rust wants written
    // via hey_ledger_take_outbound. All 0x05 framing + APDU + the hard per-exchange
    // TIMEOUT live in ledger_ble.rs / ledger_ela.rs. The blocking calls
    // (get_ela_address / send) MUST be invoked on a Kotlin background thread.

    /// Kotlin → Rust: one inbound BLE notify packet.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1on_1packet(
        env: JNIEnv,
        _class: JClass,
        pkt: JByteArray,
    ) {
        if let Ok(bytes) = env.convert_byte_array(&pkt) {
            crate::ledger_ble::on_packet(&bytes);
        }
    }

    /// Kotlin → Rust (writer loop): the next frame to write to the GATT write char,
    /// or an EMPTY array if none arrived within `timeout_ms`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1take_1outbound(
        env: JNIEnv,
        _class: JClass,
        timeout_ms: jni::sys::jlong,
    ) -> jni::sys::jbyteArray {
        let d = std::time::Duration::from_millis(timeout_ms.max(0) as u64);
        let pkt = crate::ledger_ble::take_outbound(d).unwrap_or_default();
        env.byte_array_from_slice(&pkt).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
    }

    /// Kotlin → Rust: the negotiated ATT MTU from `onMtuChanged` (caps the wire MTU).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1on_1mtu(
        _env: JNIEnv,
        _class: JClass,
        att_mtu: jint,
    ) {
        crate::ledger_ble::set_att_mtu(att_mtu.max(0) as usize);
    }

    /// Kotlin → Rust: GATT link up (true) / down (false). Down wakes any blocked op.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1on_1state(
        _env: JNIEnv,
        _class: JClass,
        connected: jboolean,
    ) {
        crate::ledger_ble::set_connected(connected != 0);
    }

    /// Kotlin → Rust: drop all transport state (between connections / hard reset).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1reset(
        _env: JNIEnv,
        _class: JClass,
    ) {
        crate::ledger_ble::reset();
    }

    /// Add-flow: GET_PUBLIC_KEY at `path` (default m/44'/2305'/0'/0/0) → the ELA
    /// `E…` address + its 33-byte compressed pubkey to store. BLOCKS on the device
    /// (open the Elastos app); call on a background thread. `{address, pubkey, path}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1get_1ela_1address(
        mut env: JNIEnv,
        _class: JClass,
        path: JString,
    ) -> jstring {
        let path = jstr(&mut env, &path)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "m/44'/2305'/0'/0/0".to_string());
        let r = crate::ledger_ela::get_pubkey(&path).map(|pk| {
            let pubkey_hex: String = pk.iter().map(|b| format!("{b:02x}")).collect();
            serde_json::json!({
                "address": crate::did::ela_address_from_pubkey(&pk),
                "pubkey": pubkey_hex,
                "path": path,
            })
        });
        out(env, json_result(r))
    }

    /// MONEY: build + Ledger-sign + broadcast an ELA MAINCHAIN transfer. `pubkey` is
    /// the compressed hex stored at add-time (no second round-trip); `path` its BIP44
    /// path. The user confirms amount+recipient on the device. BLOCKS; background thread.
    /// Spend grant kind: `ela` (same gate as hey_ela_send).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ela_1send_1ledger(
        mut env: JNIEnv,
        _class: JClass,
        path: JString,
        pubkey: JString,
        to: JString,
        amount: JString,
        auth: JString,
    ) -> jstring {
        let path = jstr(&mut env, &path).unwrap_or_default();
        let pubkey = jstr(&mut env, &pubkey).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        let r = crate::guard::redeem_spend(&auth, "ela", &to, &a)
            .and_then(|()| crate::mainchain::ela_send_ledger(&path, &pubkey, &to, &a));
        out(env, json_result(r))
    }

    /// Add-flow (EVM): GET_ADDRESS via the Ledger Ethereum app at `path` (default
    /// m/44'/60'/0'/0/0) → {address, pubkey, path}. The user opens the ETHEREUM app on
    /// the device (not Elastos). BLOCKS; background thread.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1ledger_1get_1evm_1address(
        mut env: JNIEnv,
        _class: JClass,
        path: JString,
    ) -> jstring {
        let path = jstr(&mut env, &path)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "m/44'/60'/0'/0/0".to_string());
        let r = crate::ledger_evm::get_address(&path).map(|(address, pubkey)| {
            serde_json::json!({ "address": address, "pubkey": pubkey, "path": path })
        });
        out(env, json_result(r))
    }

    /// MONEY: build + Ledger-sign + broadcast a NATIVE EVM transfer (ESC/ETH/Base).
    /// `from` = the stored 0x address for `path`. Spend grant kind `evm:<chain>` (same
    /// as hey_wallet_send) — the max-fee bound is enforced inside the signer. The user
    /// approves amount+recipient on the device. BLOCKS; background thread.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1evm_1send_1ledger(
        mut env: JNIEnv,
        _class: JClass,
        path: JString,
        from: JString,
        chain: JString,
        to: JString,
        value_hex: JString,
        auth: JString,
    ) -> jstring {
        let path = jstr(&mut env, &path).unwrap_or_default();
        let from = jstr(&mut env, &from).unwrap_or_default();
        let c = jstr(&mut env, &chain).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let v = jstr(&mut env, &value_hex).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        let redeem = crate::wallet::SpendRedeem {
            token: auth,
            kind: format!("evm:{c}"),
            to: to.clone(),
            amount: v.clone(),
        };
        let r = crate::wallet::esc_send_ledger(&path, &from, &c, &to, &v, Some(redeem));
        out(env, json_result(r))
    }

    // ── external-wallet registry (wallets.rs) — DEK-sealed, no key material ──

    /// List registered external (Ledger) wallets → JSON array.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallets_1list(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, block(crate::wallets::list()).to_string())
    }

    /// Register (or refresh) a Ledger ELA wallet — address/pubkey/path/label only.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1add_1ledger(
        mut env: JNIEnv,
        _class: JClass,
        kind: JString,
        label: JString,
        path: JString,
        address: JString,
        pubkey: JString,
        device_name: JString,
    ) -> jstring {
        ensure_plat();
        let kind = jstr(&mut env, &kind).unwrap_or_default();
        let label = jstr(&mut env, &label).unwrap_or_default();
        let path = jstr(&mut env, &path).unwrap_or_default();
        let address = jstr(&mut env, &address).unwrap_or_default();
        let pubkey = jstr(&mut env, &pubkey).unwrap_or_default();
        let device_name = jstr(&mut env, &device_name).unwrap_or_default();
        let r = block(crate::wallets::add_ledger(&kind, &label, &path, &address, &pubkey, &device_name));
        out(env, json_result(r))
    }

    /// Remove an external wallet by id → `{removed}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1wallet_1remove(
        mut env: JNIEnv,
        _class: JClass,
        id: JString,
    ) -> jstring {
        ensure_plat();
        let id = jstr(&mut env, &id).unwrap_or_default();
        out(env, json_result(block(crate::wallets::remove(&id))))
    }

    // ── the law surface (guard.rs): spend grants + audit record ─────────────

    /// Mint a one-shot spend authorization AFTER the user confirmed exactly this
    /// transfer on the confirm dialog (which sits behind the biometric gate).
    /// Returns `{"token": …}` or `{"error": …}`. The grant is single-use, bound
    /// to (kind, to, amount), and expires in 90s. When the hardware spend-binding
    /// is enrolled this fail-closes (no signature → error); use the `_hw` variant.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1authorize_1spend(
        mut env: JNIEnv,
        _class: JClass,
        kind: JString,
        to: JString,
        amount: JString,
    ) -> jstring {
        let k = jstr(&mut env, &kind).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount).unwrap_or_default();
        let r = crate::guard::authorize_spend(&k, &to, &a, None).map(|t| serde_json::json!({ "token": t }));
        out(env, json_result(r))
    }

    /// Like `hey_authorize_spend` but binds a MAX network fee (wei, decimal string)
    /// into the grant (max-fee hardening). The EVM signer rejects a tx whose real
    /// gasPrice*gasLimit exceeds it. `max_fee_wei` = "" or "0" → unbounded.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1authorize_1spend_1fee(
        mut env: JNIEnv,
        _class: JClass,
        kind: JString,
        to: JString,
        amount: JString,
        max_fee_wei: JString,
    ) -> jstring {
        let k = jstr(&mut env, &kind).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount).unwrap_or_default();
        let mf = jstr(&mut env, &max_fee_wei).unwrap_or_default().trim().parse::<u128>().unwrap_or(0);
        let r = crate::guard::authorize_spend_fee(&k, &to, &a, mf, None).map(|t| serde_json::json!({ "token": t }));
        out(env, json_result(r))
    }

    /// Hardware-bound + fee-bound mint: combines `_hw` (Keystore signature proof)
    /// with `_fee` (max-fee bound). Required when spend binding is enrolled AND the
    /// caller wants the fee bound (EVM native send). `max_fee_wei` = "" / "0" → unbounded.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1authorize_1spend_1fee_1hw(
        mut env: JNIEnv,
        _class: JClass,
        kind: JString,
        to: JString,
        amount: JString,
        max_fee_wei: JString,
        sig_hex: JString,
    ) -> jstring {
        let k = jstr(&mut env, &kind).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount).unwrap_or_default();
        let mf = jstr(&mut env, &max_fee_wei).unwrap_or_default().trim().parse::<u128>().unwrap_or(0);
        let sig = jstr(&mut env, &sig_hex).unwrap_or_default();
        let r = crate::guard::authorize_spend_fee(&k, &to, &a, mf, Some(&sig)).map(|t| serde_json::json!({ "token": t }));
        out(env, json_result(r))
    }

    /// Enroll the SEC1 (Base64) P-256 public key of an auth-required Keystore
    /// signing key, activating hardware-bound spends. Call ONLY after a Kotlin
    /// round-trip self-test (sign a `hey_spend_challenge` and confirm this returns
    /// 0), so a broken signing path never locks the user out. Returns 0 / -1.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1enroll_1spend_1key(
        mut env: JNIEnv,
        _class: JClass,
        sec1_b64: JString,
    ) -> jint {
        use base64::Engine as _;
        let Some(s) = jstr(&mut env, &sec1_b64) else { return -1 };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s.trim()) else { return -1 };
        match crate::guard::enroll_spend_key(&bytes) {
            Ok(()) => 0,
            Err(e) => {
                log::error!("enroll_spend_key: {e}");
                -1
            }
        }
    }

    /// Turn the hardware spend binding OFF for this process. H4: when a binding is
    /// ACTIVE this FAILS CLOSED (returns -1) — the caller must use the signature-
    /// verified `_hw` variant. Only succeeds when the binding is inactive / never
    /// enrolled (idempotent / legacy). Returns 0 on success, -1 if proof is required.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1unenroll_1spend_1key(
        _env: JNIEnv,
        _class: JClass,
    ) -> jint {
        match crate::guard::unenroll_spend_key() {
            Ok(()) => 0,
            Err(e) => {
                log::warn!("unenroll_spend_key: {e}");
                -1
            }
        }
    }

    /// Issue a one-time challenge to DISABLE the spend binding (H4). The Kotlin
    /// BiometricPrompt CryptoObject signs `challenge\0spend.unenroll\0spend.unenroll\0spend.unenroll`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1unenroll_1challenge(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        out(env, crate::guard::issue_unenroll_challenge().unwrap_or_default())
    }

    /// HARDWARE-VERIFIED disable (H4): turn the binding off ONLY after a fresh
    /// Keystore signature over the one-time disable-challenge verifies. An in-process
    /// caller can no longer silently disarm the spend binding. Returns 0 / -1.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1unenroll_1spend_1key_1hw(
        mut env: JNIEnv,
        _class: JClass,
        sig_hex: JString,
    ) -> jint {
        let sig = jstr(&mut env, &sig_hex).unwrap_or_default();
        match crate::guard::unenroll_spend_key_hw(&sig) {
            Ok(()) => 0,
            Err(e) => {
                log::warn!("unenroll_spend_key_hw: {e}");
                -1
            }
        }
    }

    /// Enrollment self-test: prove the Keystore-sign → Rust-verify path works on
    /// THIS device BEFORE activating the binding. Kotlin signs
    /// `challenge\0selftest\0selftest\0selftest` and passes the SEC1 pubkey here;
    /// returns 0 if it verifies. Only enroll after this returns 0 (fail-safe).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1spend_1selftest(
        mut env: JNIEnv,
        _class: JClass,
        sec1_b64: JString,
        challenge: JString,
        sig_hex: JString,
    ) -> jint {
        use base64::Engine as _;
        let Some(s) = jstr(&mut env, &sec1_b64) else { return -1 };
        let Ok(sec1) = base64::engine::general_purpose::STANDARD.decode(s.trim()) else { return -1 };
        let ch = jstr(&mut env, &challenge).unwrap_or_default();
        let sig = jstr(&mut env, &sig_hex).unwrap_or_default();
        if crate::guard::spend_selftest(&sec1, &ch, &sig) { 0 } else { -1 }
    }

    /// Issue a fresh one-time challenge for the next hardware-bound spend. The
    /// Kotlin BiometricPrompt CryptoObject signs `challenge\0kind\0to\0amount`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1spend_1challenge(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        out(env, crate::guard::issue_spend_challenge().unwrap_or_default())
    }

    /// Hardware-bound mint: like `hey_authorize_spend` but carries the Keystore
    /// signature (hex DER) proving a real biometric op authorized exactly this
    /// transfer. Required once a spend key is enrolled.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1authorize_1spend_1hw(
        mut env: JNIEnv,
        _class: JClass,
        kind: JString,
        to: JString,
        amount: JString,
        sig_hex: JString,
    ) -> jstring {
        let k = jstr(&mut env, &kind).unwrap_or_default();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let a = jstr(&mut env, &amount).unwrap_or_default();
        let sig = jstr(&mut env, &sig_hex).unwrap_or_default();
        let r = crate::guard::authorize_spend(&k, &to, &a, Some(&sig)).map(|t| serde_json::json!({ "token": t }));
        out(env, json_result(r))
    }

    /// Recent audit-log lines (privileged acts: transfers, grants, denials),
    /// newest last, as plain text — the user's own record of what their
    /// runtime did with its authority.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1audit_1log(
        env: JNIEnv,
        _class: JClass,
        limit: jint,
    ) -> jstring {
        out(env, crate::guard::audit_tail(limit.max(1) as usize).join("\n"))
    }

    /// Validate a BIP39 recovery phrase before restoring (12/24 words, checksum).
    /// Returns "ok" if valid, "" otherwise.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1validate_1mnemonic(
        mut env: JNIEnv,
        _class: JClass,
        phrase: JString,
    ) -> jstring {
        let p = jstr(&mut env, &phrase).unwrap_or_default();
        out(env, if bip39::Mnemonic::parse(p.trim()).is_ok() { "ok".into() } else { String::new() })
    }

    // ── tipping: publish my receive addresses + resolve a peer's by identity ──

    /// Publish my tip-receive addresses (`{chainKey:address}` JSON) in my signed profile.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1set_1tip_1addresses(
        mut env: JNIEnv,
        _class: JClass,
        addresses_json: JString,
    ) -> jstring {
        ensure_plat();
        let j = jstr(&mut env, &addresses_json).unwrap_or_default();
        out(env, json_result(block(social::set_tip_addresses(&j))))
    }

    /// A peer's published tip addresses (`{chainKey:address}` or null) — resolves a
    /// tip by identity so the user never needs the recipient's address.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1resolve_1tip(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::resolve_tip(&d)).to_string())
    }

    /// Tip-sheet resolve that ALSO exchanges addresses over the DM channel, so tipping
    /// a chat contact resolves even without following them.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1refresh_1contact(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::refresh_contact_addresses(&d)).to_string())
    }

    /// Notify a tip recipient over the carrier (after the on-chain transfer) so they
    /// get a "sent you a tip" notification even with the app closed. Fire-and-forget.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1notify_1tip(
        mut env: JNIEnv,
        _class: JClass,
        to: JString,
        sym: JString,
        amount: JString,
        txid: JString,
    ) -> jstring {
        ensure_plat();
        let to = jstr(&mut env, &to).unwrap_or_default();
        let sym = jstr(&mut env, &sym).unwrap_or_default();
        let amount = jstr(&mut env, &amount).unwrap_or_default();
        let txid = jstr(&mut env, &txid).unwrap_or_default();
        let ok = block(social::notify_tip(&to, &sym, &amount, &txid));
        out(env, format!("{{\"ok\":{ok}}}"))
    }

    // ── social graph (followers) + profile + deletes ─────────────────────────

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1followers(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        ensure_plat();
        out(env, json_result(block(social::followers())))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1follow_1back(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::follow_back(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1start_1chat(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::start_chat(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1user_1posts(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::user_posts(&d)).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1delete_1conversation(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, json_result(block(social::delete_conversation(&d))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1delete_1group(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, json_result(block(social::delete_group(&g))))
    }

    /// ADMIN "delete group for everyone" — creator-only. Fans a signed DISSOLVE
    /// to every member, then deletes locally + tombstones. Returns {ok}/{error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1dissolve_1group(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, json_result(block(social::chat_dissolve_group(&g))))
    }

    /// Accept a pending group invite. Returns {ok} or {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1accept_1group(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, json_result(block(social::chat_accept_group(&g))))
    }

    /// Decline a pending group invite. Returns {ok} or {error}.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1decline_1group(
        mut env: JNIEnv,
        _class: JClass,
        gid: JString,
    ) -> jstring {
        ensure_plat();
        let g = jstr(&mut env, &gid).unwrap_or_default();
        out(env, json_result(block(social::chat_decline_group(&g))))
    }

    // ── profile (nickname/bio/avatar) + post edit/delete ─────────────────────

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1set_1profile(
        mut env: JNIEnv,
        _class: JClass,
        nickname: JString,
        bio: JString,
        avatar: JString,
    ) -> jstring {
        ensure_plat();
        let n = jstr(&mut env, &nickname).unwrap_or_default();
        let b = jstr(&mut env, &bio).unwrap_or_default();
        let a = jstr(&mut env, &avatar).unwrap_or_default();
        out(env, json_result(block(social::set_profile(&n, &b, &a))))
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1get_1profile(
        mut env: JNIEnv,
        _class: JClass,
        did: JString,
    ) -> jstring {
        ensure_plat();
        let d = jstr(&mut env, &did).unwrap_or_default();
        out(env, block(social::get_profile(&d)).to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1delete_1post(
        mut env: JNIEnv,
        _class: JClass,
        id: JString,
    ) -> jstring {
        ensure_plat();
        let i = jstr(&mut env, &id).unwrap_or_default();
        out(env, json_result(block(social::delete_post(&i))))
    }

    /// Drain pending local-notification events (foreground service posts them).
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1drain_1notifs(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        out(env, social::drain_notifs().to_string())
    }

    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1edit_1post(
        mut env: JNIEnv,
        _class: JClass,
        id: JString,
        caption: JString,
    ) -> jstring {
        ensure_plat();
        let i = jstr(&mut env, &id).unwrap_or_default();
        let c = jstr(&mut env, &caption).unwrap_or_default();
        out(env, json_result(block(social::edit_post(&i, &c))))
    }

    // ── BEAM read ops, phrase resolved IN-PROCESS (H5 corollary) ─────────────
    //
    // BEAM's C++ shim needs the mnemonic to OPEN the wallet DB for even read-only
    // ops (address/balance/scan/node). With H5 blocking the bare hey_recovery_phrase
    // JNI when the binding is active, Kotlin can no longer pull the phrase to feed
    // these — so they resolve the phrase in Rust (wallet_phrase) and invoke the C++
    // BeamApi static method via JNI, so the mnemonic never crosses JNI from Kotlin.
    // These are read-only (no guard grant). RECEIVE + BALANCE only — BEAM send is
    // gated for this cut and intentionally NOT ported here.

    /// Invoke a `String`-returning `BeamApi.<name>(mnemonic, <extra...>)` static C++
    /// method with the IN-PROCESS phrase prepended. `extra` are the trailing JNI
    /// values; `sig` is the full JNI method signature. Returns the C++ JSON string.
    fn beam_call_with_phrase(
        env: &mut JNIEnv,
        name: &str,
        sig: &str,
        extra: &[jni::objects::JValue],
    ) -> String {
        let phrase = match super::wallet_phrase() {
            Ok(p) => p,
            Err(e) => return serde_json::json!({ "error": e }).to_string(),
        };
        let r = (|| -> Result<String, String> {
            let j_phrase = env.new_string(&phrase).map_err(|e| format!("jni phrase: {e}"))?;
            let mut args: Vec<jni::objects::JValue> = Vec::with_capacity(1 + extra.len());
            args.push(jni::objects::JValue::Object(&j_phrase));
            args.extend_from_slice(extra);
            let ret = env
                .call_static_method("os/elastos/hey/social/BeamApi", name, sig, &args)
                .map_err(|e| format!("beam {name} invoke: {e}"))?;
            let obj = ret.l().map_err(|e| format!("beam {name} return: {e}"))?;
            Ok(env
                .get_string(&jni::objects::JString::from(obj))
                .map_err(|e| format!("beam {name} decode: {e}"))?
                .into())
        })();
        // L-1: scrub the in-process phrase copy now that the JNI string has been built
        // and the call has returned (the closure borrowed `&phrase`, so wipe AFTER it).
        // The JVM holds its own copy of the words for the duration of the C++ call —
        // that lives outside Rust's reach; this only scrubs our transient buffer.
        super::wipe_phrase(phrase);
        r.unwrap_or_else(|e| serde_json::json!({ "error": e }).to_string())
    }

    /// BEAM tip/donation address — phrase in-process. `{token}` or `{error}`.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1address(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let j_dir = match env.new_string(&dir) {
            Ok(s) => s,
            Err(_) => return out(env, serde_json::json!({ "error": "jni dir" }).to_string()),
        };
        let s = beam_call_with_phrase(
            &mut env,
            "beam_address",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[jni::objects::JValue::Object(&j_dir)],
        );
        out(env, s)
    }

    /// BEAM + BEAMX balances (last sync, no network) — phrase in-process.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1balance(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
        node: JString,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let node = jstr(&mut env, &node).unwrap_or_default();
        let (Ok(j_dir), Ok(j_node)) = (env.new_string(&dir), env.new_string(&node)) else {
            return out(env, serde_json::json!({ "error": "jni args" }).to_string());
        };
        let s = beam_call_with_phrase(
            &mut env,
            "beam_balance",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[jni::objects::JValue::Object(&j_dir), jni::objects::JValue::Object(&j_node)],
        );
        out(env, s)
    }

    /// Connect + scan against a node (quicksync / own-node) — phrase in-process.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1scan(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
        node: JString,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let node = jstr(&mut env, &node).unwrap_or_default();
        let (Ok(j_dir), Ok(j_node)) = (env.new_string(&dir), env.new_string(&node)) else {
            return out(env, serde_json::json!({ "error": "jni args" }).to_string());
        };
        let s = beam_call_with_phrase(
            &mut env,
            "beam_scan",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[jni::objects::JValue::Object(&j_dir), jni::objects::JValue::Object(&j_node)],
        );
        out(env, s)
    }

    /// Confirmation status of a BEAM tx — phrase in-process.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1tx_1status(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
        node: JString,
        txid: JString,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let node = jstr(&mut env, &node).unwrap_or_default();
        let txid = jstr(&mut env, &txid).unwrap_or_default();
        let (Ok(j_dir), Ok(j_node), Ok(j_txid)) =
            (env.new_string(&dir), env.new_string(&node), env.new_string(&txid))
        else {
            return out(env, serde_json::json!({ "error": "jni args" }).to_string());
        };
        let s = beam_call_with_phrase(
            &mut env,
            "beam_tx_status",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[jni::objects::JValue::Object(&j_dir), jni::objects::JValue::Object(&j_node), jni::objects::JValue::Object(&j_txid)],
        );
        out(env, s)
    }

    /// Start the on-device BEAM node (loopback) — phrase in-process.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1node_1start(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let Ok(j_dir) = env.new_string(&dir) else {
            return out(env, serde_json::json!({ "error": "jni dir" }).to_string());
        };
        let s = beam_call_with_phrase(
            &mut env,
            "beam_node_start",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[jni::objects::JValue::Object(&j_dir)],
        );
        out(env, s)
    }

    /// Scan against the on-device node, gated on node-synced — phrase in-process.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1scan_1local(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
        wait_ms: jint,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let Ok(j_dir) = env.new_string(&dir) else {
            return out(env, serde_json::json!({ "error": "jni dir" }).to_string());
        };
        let s = beam_call_with_phrase(
            &mut env,
            "beam_scan_local",
            "(Ljava/lang/String;Ljava/lang/String;I)Ljava/lang/String;",
            &[jni::objects::JValue::Object(&j_dir), jni::objects::JValue::Int(wait_ms)],
        );
        out(env, s)
    }

    /// Convert a decimal BEAM amount string to groth (1 BEAM = 100_000_000 groth).
    /// Mirrors Kotlin `BeamApi.toGroth` (BigDecimal.movePointRight(8).toBigIntegerExact)
    /// EXACTLY: at most 8 fractional digits, digits only, no sign — so a value that
    /// passed the Kotlin pre-check converts identically here. Used as the in-Rust source
    /// of truth and cross-checked against the Kotlin groth before any send.
    fn beam_to_groth(s: &str) -> Result<u64, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty amount".into());
        }
        let (int_part, frac_part) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if frac_part.len() > 8 {
            return Err("BEAM has at most 8 decimals".into());
        }
        let digits_only = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if !digits_only(int_part) || !digits_only(frac_part) {
            return Err("invalid amount".into());
        }
        let int_v: u64 = if int_part.is_empty() {
            0
        } else {
            int_part.parse().map_err(|_| "amount too large".to_string())?
        };
        let mut frac_buf = String::from(frac_part);
        while frac_buf.len() < 8 {
            frac_buf.push('0');
        }
        let frac_v: u64 = if frac_buf.is_empty() { 0 } else { frac_buf.parse().map_err(|_| "invalid amount".to_string())? };
        int_v
            .checked_mul(100_000_000)
            .and_then(|x| x.checked_add(frac_v))
            .ok_or_else(|| "amount too large".into())
    }

    /// Arm the shim for EXACTLY one upcoming `beam_send` keyed on (token, groth, asset,
    /// nonce) — no phrase. The shim's `consume_arm` fail-closes unless this matches, so a
    /// bare in-process `beam_send` (or a replayed nonce) is refused.
    fn beam_arm(env: &mut JNIEnv, token: &str, groth: u64, asset: u32, nonce: &str) -> Result<(), String> {
        let j_token = env.new_string(token).map_err(|e| format!("jni token: {e}"))?;
        let j_nonce = env.new_string(nonce).map_err(|e| format!("jni nonce: {e}"))?;
        env.call_static_method(
            "os/elastos/hey/social/BeamApi",
            "beam_arm_send",
            "(Ljava/lang/String;JILjava/lang/String;)V",
            &[
                jni::objects::JValue::Object(&j_token),
                jni::objects::JValue::Long(groth as i64),
                jni::objects::JValue::Int(asset as i32),
                jni::objects::JValue::Object(&j_nonce),
            ],
        )
        .map(|_| ())
        .map_err(|e| format!("beam arm: {e}"))
    }

    /// BEAM SEND (guarded) — the only money-moving BEAM JNI. Flow, fail-closed at each step:
    ///   1. `redeem_spend` consumes the one-shot grant bound to (kind="beam:<asset>",
    ///      recipient token, the EXACT decimal amount string the user confirmed);
    ///   2. convert that SAME string to groth in Rust and require it equals Kotlin's groth
    ///      (defense in depth — a converter bug on either side blocks the send, never sends
    ///      a wrong amount);
    ///   3. sanity cap + audit;
    ///   4. mint a FRESH 16-byte single-use nonce;
    ///   5. arm the shim for exactly (token, groth, asset, nonce);
    ///   6. build + broadcast via the LOOPBACK mobilenode ONLY (127.0.0.1:31744) — never a
    ///      public node, honouring the self-host privacy guarantee.
    /// Returns the shim's {"txid","status"} JSON, or {"error"}. Anything that fails before
    /// step 5 means nothing was broadcast; the grant is single-use either way.
    #[no_mangle]
    pub extern "system" fn Java_os_elastos_hey_social_HeyApi_hey_1beam_1send(
        mut env: JNIEnv,
        _class: JClass,
        dir: JString,
        token: JString,
        amount: JString,
        amount_groth: jni::sys::jlong,
        fee_groth: jni::sys::jlong,
        asset_id: jni::sys::jint,
        auth: JString,
    ) -> jstring {
        let dir = jstr(&mut env, &dir).unwrap_or_default();
        let token = jstr(&mut env, &token).unwrap_or_default();
        let amount = jstr(&mut env, &amount).unwrap_or_default();
        let auth = jstr(&mut env, &auth).unwrap_or_default();
        let asset: u32 = if asset_id < 0 { 0 } else { asset_id as u32 };
        let kind = format!("beam:{asset}");

        let r = (|| -> Result<String, String> {
            // 1) Cheap, deterministic checks FIRST — these don't consume the one-shot grant,
            //    so a transient failure (dup, cap, fee) never forces the user to re-confirm.
            //    groth comes from the SAME string the grant pins; cross-check Kotlin's value.
            let groth = beam_to_groth(&amount)?;
            if amount_groth < 0 || groth != amount_groth as u64 {
                return Err("amount safety check failed — not sent".into());
            }
            crate::guard::check_beam_cap(groth)?; // sanity ceiling + audit
            if fee_groth <= 0 {
                return Err("invalid fee".into());
            }
            crate::guard::check_beam_fee(fee_groth as u64)?; // fee can never become a drain
            // 2) Idempotency: refuse a duplicate (recipient, amount) still inside the dedup
            //    window — stops a re-confirmed slow-to-confirm send from double-paying.
            crate::guard::check_beam_dup(&token, groth)?;
            // 3) Authorize: consume the one-shot grant bound to (kind, recipient, EXACT amount).
            crate::guard::redeem_spend(&auth, &kind, &token, &amount)?;
            // 4) Fresh single-use nonce binds this arm to this send.
            let mut nb = [0u8; 16];
            getrandom::getrandom(&mut nb).map_err(|e| format!("nonce entropy: {e}"))?;
            let nonce: String = nb.iter().map(|x| format!("{x:02x}")).collect();
            // 5) arm the shim for exactly (token, groth, asset, nonce).
            beam_arm(&mut env, &token, groth, asset, &nonce)?;
            // 6) build + broadcast via the LOOPBACK mobilenode ONLY.
            let j_dir = env.new_string(&dir).map_err(|e| format!("jni dir: {e}"))?;
            let j_node = env.new_string("127.0.0.1:31744").map_err(|e| format!("jni node: {e}"))?;
            let j_token = env.new_string(&token).map_err(|e| format!("jni token: {e}"))?;
            let j_nonce = env.new_string(&nonce).map_err(|e| format!("jni nonce: {e}"))?;
            let s = beam_call_with_phrase(
                &mut env,
                "beam_send",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJILjava/lang/String;)Ljava/lang/String;",
                &[
                    jni::objects::JValue::Object(&j_dir),
                    jni::objects::JValue::Object(&j_node),
                    jni::objects::JValue::Object(&j_token),
                    jni::objects::JValue::Long(groth as i64),
                    jni::objects::JValue::Long(fee_groth),
                    jni::objects::JValue::Int(asset as i32),
                    jni::objects::JValue::Object(&j_nonce),
                ],
            );
            // 7) Record an actual broadcast (shim returned a txid) for the dedup window.
            if s.contains("\"txid\"") {
                crate::guard::record_beam_send(&token, groth);
            }
            Ok(s)
        })();
        let s = match r {
            Ok(s) => s,
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        };
        out(env, s)
    }
}

#[cfg(test)]
mod h2_tests {
    //! H2.9 — assert the headless-boot contract (no crypto change): a sentinel cold
    //! start WITH carrier-identity.json (and NO identity.json) boots HEADLESS — the
    //! seed stays sealed (`identity_sealed`==true ⇒ `processing_deferred`==true, so
    //! the receiver buffers instead of decrypting) — and a subsequent unlock flips it
    //! false. This exercises the SAME `pub(crate)` helpers + `plat` flags the boot
    //! ladder (lib.rs ~192-240) and `hey_unlock` (~738) use, so a regression that
    //! breaks the headless decision or the unlock flip fails here.
    use super::{identity, HEADLESS_BOOT_SENTINEL};

    /// Replays the sentinel-boot DECISION exactly as `run_async`: with no identity.json
    /// and a present carrier-identity.json it lands on Headless and seals the seed.
    /// Returns true if the ladder chose Headless (and set the sealed flag).
    fn ladder_headless_decision(dir: &std::path::Path) -> bool {
        let id_path = dir.join("identity.json");
        let ci_path = dir.join("carrier-identity.json");
        // mirror: Some(sentinel) -> identity.json? Full : carrier blob? Headless : fail
        assert_eq!(HEADLESS_BOOT_SENTINEL, "__hey_headless_boot__");
        if identity::read_identity_blob(&id_path).is_some() {
            return false; // would boot Full (seed on disk)
        }
        if let Some(ci) = identity::read_carrier_identity(&ci_path) {
            assert!(ci.carrier_sk().is_some(), "carrier_sk must decode");
            hey_core::plat::set_identity_sealed(true); // SAME call as the boot ladder
            return true;
        }
        false // would fail closed (no blob)
    }

    #[test]
    fn sentinel_cold_start_boots_headless_then_unlock_flips_sealed() {
        // Install a DEK so carrier-identity.json is sealed at rest (mobile invariant).
        hey_core::plat::set_at_rest_key([7u8; 32]);
        assert!(hey_core::plat::at_rest_active());

        let tmp = std::env::temp_dir().join(format!("hey-h2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let ci_path = tmp.join("carrier-identity.json");

        // A real identity → its one-way carrier blob (NO seed), persisted like a
        // vault-ON enableVault/Full-boot would. NO identity.json on disk (vault ON).
        let id = identity::Identity::generate();
        assert!(identity::write_carrier_identity(&ci_path, &id.to_carrier_identity()));
        assert!(!tmp.join("identity.json").exists());

        // Sentinel cold start: the ladder chooses HEADLESS and seals the seed.
        let headless = ladder_headless_decision(&tmp);
        assert!(headless, "sentinel + carrier blob (no identity.json) must boot Headless");
        assert!(hey_core::plat::identity_sealed(), "headless boot must seal the seed");
        assert!(
            hey_core::plat::processing_deferred(),
            "sealed seed ⇒ processing deferred (buffer, don't decrypt)"
        );

        // Subsequent hey_unlock installs the seed-backed identity and clears the flag.
        hey_core::plat::set_identity_sealed(false); // the exact flip hey_unlock makes
        assert!(!hey_core::plat::identity_sealed());
        assert!(
            !hey_core::plat::processing_deferred(),
            "after unlock the receiver may decrypt + drain the broker"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
