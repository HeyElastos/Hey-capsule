//! The eframe::App: drains engine/receiver events into AppState, paints the frost
//! background + top bar + tab body + floating dock, and owns the polling cadence
//! and the per-action dispatch helpers the views call.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use egui::{Align2, Color32, FontId, RichText};
use serde_json::Value;

use crate::engine::Engine;
use crate::media::MediaCache;
use crate::runtime_boot::{start_receivers, Boot};
use crate::state::{as_array, AppState, CallState, Modal, OpenChat, Tab, UiEvent};
use crate::theme::{Theme, GOLD, LIKE};
use crate::views;

/// Top safe-area so macOS traffic-lights never overlap our content. On macOS the
/// titlebar is transparent (fullsize content view) so we pad the sidebar + the
/// content header down by this much; everywhere else a small 8px breathing inset.
pub const TOP_INSET: f32 = if cfg!(target_os = "macos") { 28.0 } else { 8.0 };

/// iPad-sheet entrance offset (§5g): the y-offset a centered sheet should add to
/// its `CENTER_CENTER` anchor so it slides up into place. `overlays()` writes the
/// shared `"sheet-anim"` (0→1 over 0.16s) into ctx.data each frame; sheet bodies
/// (owned by the view/sheet agents) call this to get the per-frame rise, e.g.:
/// `.anchor(Align2::CENTER_CENTER, vec2(0.0, crate::app::sheet_rise(ctx)))`.
pub fn sheet_rise(ctx: &egui::Context) -> f32 {
    let anim = ctx
        .data(|d| d.get_temp::<f32>(egui::Id::new("sheet-anim")))
        .unwrap_or(1.0);
    (1.0 - anim) * 20.0
}

pub struct App {
    pub engine: Engine,
    pub media: MediaCache,
    pub ev_tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
    pub state: AppState,
    // polling deadlines (seconds since app start)
    next_health: f64,
    next_chats: f64,
    next_convo: f64,
    next_activity: f64,
    next_call_poll: f64,
    call_since: Option<std::time::Instant>,
    // The live cpal audio pump + (for video calls) the camera/decode threads for
    // the current Active call. Some only between start_media and stop_media. Held
    // here because cpal's `Stream` is `!Send` and must live on the UI thread.
    call_media: Option<crate::call_media::CallMedia>,
    // Dev-only self-capture (HEY_SHOT=path): grab the GL framebuffer once and exit.
    shot: Option<String>,
    shot_requested: bool,
    // One-shot guard so an identity relaunch (WalletSeedCreated / OnboardRestored)
    // can never spawn a second process or re-enter the exit path mid-frame.
    relaunching: bool,
    // One-shot guard so my receive (tip) addresses are published to my profile only
    // once per run (== Android's `tips_published` pref). Set when the publish is
    // dispatched on the first wallet-addresses load.
    tips_published: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, boot: Boot) -> Self {
        let ctx = cc.egui_ctx.clone();
        ctx.set_pixels_per_point(1.0); // EXPLICIT real-pointer scale — desktop instrument, not tablet-chunky
        crate::icons::setup(&ctx);
        Theme::get(false).apply(&ctx); // DARK is the hero default (P2P/crypto tool earns it)

        let (ev_tx, rx) = channel::<UiEvent>();
        let engine = Engine::new(boot.port, boot.store.clone(), ctx.clone(), 3);
        start_receivers(boot.port, boot.store, ctx, ev_tx.clone());

        let mut app = App {
            engine,
            media: MediaCache::default(),
            ev_tx,
            rx,
            state: AppState::default(),
            next_health: 0.0,
            next_chats: 0.0,
            next_convo: 0.0,
            next_activity: 0.0,
            next_call_poll: 0.0,
            call_since: None,
            call_media: None,
            shot: std::env::var("HEY_SHOT").ok(),
            shot_requested: false,
            relaunching: false,
            tips_published: false,
        };
        // Theme: a persisted Light/Dark choice (theme.txt, parity with Android)
        // wins; otherwise DARK is the hero default (HEY_LIGHT forces light).
        app.state.light = crate::theme::load_pref()
            .unwrap_or_else(|| std::env::var("HEY_LIGHT").is_ok());
        // First run shows the welcome flow until the user picks create-new / restore.
        app.state.onboarded = std::path::Path::new(&app.engine.store).join(".hey-onboarded").exists();
        if app.shot.is_some() {
            app.state.tab = match std::env::var("HEY_SHOT_TAB").as_deref() {
                Ok("chat") => Tab::Chat,
                Ok("feed") => Tab::Feed,
                Ok("wallet") => Tab::Wallet,
                Ok("verse") => Tab::Verse,
                Ok("activity") => Tab::Activity,
                _ => Tab::Profile,
            };
            // Dev: open a modal for screenshot verification (HEY_SHOT_MODAL=edit|addfriend|composer).
            app.state.modal = match std::env::var("HEY_SHOT_MODAL").as_deref() {
                Ok("edit") => Some(Modal::EditProfile),
                Ok("addfriend") => Some(Modal::AddFriend),
                Ok("composer") => Some(Modal::Composer),
                Ok("newgroup") => Some(Modal::NewGroup),
                Ok("connection") => Some(Modal::Connection),
                _ => None,
            };
        }
        app.load_chat_prefs(); // muted_chats / blocked_dids (== Android SharedPreferences)
        app.load_whoami();
        app.load_friend_link();
        app.load_follow_link(); // slim hyper:follow QR (profile)
        app.load_chat_link(); // slim hyper:chat QR (new chat)
        app.load_profile();
        app.load_feed();
        app.load_chats();
        app.load_wallet_history();
        app.load_hidden_tokens();
        app
    }

    // ── dispatch helpers ──────────────────────────────────────────────────────

    pub fn load_whoami(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::whoami().await },
            |r| match r {
                Ok(v) => UiEvent::Whoami {
                    did: v.get("did").and_then(Value::as_str).unwrap_or("").to_string(),
                },
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn load_friend_link(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::my_friend_link().await },
            |r| match r {
                Ok(s) => UiEvent::FriendLink(s),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// Slim FOLLOW QR link (hyper:follow:) for the profile — full PQ keys, ~30% smaller.
    pub fn load_follow_link(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::my_follow_link().await },
            |r| match r {
                Ok(s) => UiEvent::FollowLink(s),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// Slim CHAT QR link (hyper:chat:) for New chat — full PQ keys, ~30% smaller.
    pub fn load_chat_link(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::my_chat_link().await },
            |r| match r {
                Ok(s) => UiEvent::ChatLink(s),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// ISOLATION: fetch whether a private chat with `did` is permitted (chat established, not just a
    /// follow). Drives the composer / Message-button gate; the engine enforces it regardless.
    pub fn fetch_can_chat(&self, did: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let ok = hey_mobile_runtime::social::can_chat(&did).await;
                Ok::<_, String>((did, ok))
            },
            |r| match r {
                Ok((did, ok)) => UiEvent::CanChat { did, ok },
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn load_health(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::carrier_health().await },
            |v| UiEvent::Health {
                online: v.get("online").and_then(Value::as_bool).unwrap_or(false),
                direct: v.get("direct").and_then(Value::as_bool).unwrap_or(false),
                direct_peers: v.get("direct_peers").and_then(Value::as_i64).unwrap_or(0),
                relay_peers: v.get("relay_peers").and_then(Value::as_i64).unwrap_or(0),
                peers: v.get("peer_count").and_then(Value::as_i64).unwrap_or(0),
                public_v4: v.get("public_v4").and_then(Value::as_str).unwrap_or("").to_string(),
                public_v6: v.get("public_v6").and_then(Value::as_str).unwrap_or("").to_string(),
                ipv4: v.get("ipv4").and_then(Value::as_bool).unwrap_or(false),
                ipv6_global: v.get("ipv6_global").and_then(Value::as_bool).unwrap_or(false),
                udp_v4: v.get("udp_v4").and_then(Value::as_bool).unwrap_or(false),
                udp_v6: v.get("udp_v6").and_then(Value::as_bool).unwrap_or(false),
                local_addrs: v
                    .get("local_addrs")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
            },
        );
    }

    pub fn load_feed(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::feed(50).await },
            |r| match r {
                Ok(v) => UiEvent::Feed(as_array(v)),
                Err(_) => UiEvent::Feed(Vec::new()),
            },
        );
    }

    // ── wallet ────────────────────────────────────────────────────────────────

    /// Resolve the wallet addresses (EVM 0x…, ELA E…, did:elastos) + the EVM chain
    /// list. Lazy: the Wallet view calls this on first open.
    pub fn load_wallet(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { crate::walletops::addresses() },
            |r| match r {
                Ok((evm, ela, did, chains)) => UiEvent::WalletAddresses { evm, ela, did, chains },
                // A legacy identity with no BIP39 phrase → offer to create one.
                Err(e) if e.contains("locked") => UiEvent::WalletLocked,
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// Generate + persist a fresh BIP39 identity (for a desktop whose stored seed
    /// predates the wallet), then signal a restart so the runtime loads it.
    pub fn create_wallet_seed(&self) {
        let dir = self.engine.store.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::create_fresh_identity(std::path::Path::new(&dir)) },
            |r| match r {
                Ok(_) => UiEvent::WalletSeedCreated,
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// CREATE-new path: after keeping the runtime's auto-created (fresh BIP39)
    /// identity, show the one-time "Set up your profile" step (== Android
    /// `OnboardingScreen`) before entering the app. The identity already exists
    /// (auto-created at boot), so `set_profile` can run against the live identity —
    /// no relaunch is needed. RESTORE-from-phrase skips this (it already has a
    /// profile) and goes straight through `onboard_restore`.
    pub fn begin_profile_setup(&mut self) {
        // Fresh draft for the new user (no existing profile to prefill).
        self.state.profile_draft = crate::state::ProfileDraft::default();
        self.state.onboarding.profile_setup = true;
    }

    /// Submit the CREATE-new profile-setup step: upload happens via `pick_avatar`
    /// (which already sets `profile_draft.avatar_cid`), then save via the SAME
    /// `set_profile` engine fn the EditProfile sheet uses. We finish onboarding on
    /// EITHER outcome (Ok or Err) so a save failure never traps the new user on the
    /// setup screen — mirroring Android, which wraps setProfile in runCatching and
    /// always proceeds to `onDone()`.
    pub fn submit_onboard_profile(&mut self) {
        let nick = {
            let n = self.state.profile_draft.nickname.trim();
            if n.is_empty() { "Hey user".to_string() } else { n.to_string() }
        };
        let bio = self.state.profile_draft.bio.trim().to_string();
        let avatar = self.state.profile_draft.avatar_cid.clone();
        self.state.profile_draft.busy = true;
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::set_profile(&nick, &bio, &avatar).await },
            UiEvent::OnboardProfileSet,
        );
    }

    /// Finish onboarding by keeping the runtime's auto-created (fresh BIP39) identity.
    pub fn finish_onboarding(&mut self) {
        let _ = std::fs::write(std::path::Path::new(&self.engine.store).join(".hey-onboarded"), b"1");
        self.state.onboarding.profile_setup = false;
        self.state.onboarded = true;
    }

    /// Restore the identity from a recovery phrase (then restart to load it).
    pub fn onboard_restore(&self, phrase: String) {
        let dir = self.engine.store.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::restore_identity(std::path::Path::new(&dir), &phrase) },
            |r| match r {
                Ok(_) => UiEvent::OnboardRestored,
                Err(e) => UiEvent::OnboardError(e),
            },
        );
    }

    /// Fetch the balance bundle for one chain (native + curated tokens for EVM, or
    /// the mainchain balance for "ela"). On failure we still emit a balance event
    /// (carrying an `{error}`) so the refreshing spinner for that chain clears.
    pub fn load_wallet_balance(&self, chain: &str) {
        let chain = chain.to_string();
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let res = crate::walletops::balance(&chain);
                (chain, res)
            },
            |(chain, res)| match res {
                Ok(data) => UiEvent::WalletBalance { chain, data },
                Err(e) => UiEvent::WalletBalance { chain, data: serde_json::json!({ "error": e }) },
            },
        );
    }

    /// Authorize + sign + broadcast a transfer on a worker. `token` Some => ERC-20.
    pub fn wallet_send(&self, chain: String, token: Option<Value>, to: String, amount: String, to_did: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { crate::walletops::send(&chain, token.as_ref(), &to, &amount, &to_did) },
            |r| match r {
                Ok(rec) => UiEvent::WalletSent(rec),
                Err(e) => UiEvent::WalletSendFailed(e),
            },
        );
    }

    // ── tipping ─────────────────────────────────────────────────────────────────
    //
    // Mirrors the Android tip flow 1:1 over the SAME shared engine fns:
    //   provision: social::set_tip_addresses(json)  (== Android publishTipAddresses)
    //   resolve:   social::refresh_contact_addresses(did) -> {chainKey: address}
    //   send:      walletops::send(.., to_did)  (the EXISTING wallet send path, tags kind:"tip")
    //   notify:    social::notify_tip(did, sym, amount, txid)  (DM "sent you a tip")

    /// Open the Tip sheet for a recipient (a feed author, a chat contact, or a
    /// profile). Resets the form, marks it Resolving, and kicks off the by-identity
    /// address resolve. The recipient's `did`/`name` come from the call site.
    pub fn open_tip(&mut self, did: &str, name: &str) {
        if did.trim().is_empty() {
            return;
        }
        // Make sure the wallet is loaded so we know the sender's chain registry +
        // can sign; lazy-load it if the user hasn't opened the Wallet tab yet.
        if !self.state.wallet.loaded && !self.state.wallet.locked {
            self.load_wallet();
        }
        self.state.tip = crate::state::TipForm {
            open: true,
            did: did.to_string(),
            name: if name.trim().is_empty() {
                crate::state::AppState::short_did(did)
            } else {
                name.to_string()
            },
            stage: crate::state::TipStage::Resolving,
            ..Default::default()
        };
        self.resolve_tip(did);
    }

    /// Publish MY receive addresses so others can tip me by identity (no address
    /// sharing). Mirrors Android `publishTipAddresses`: every EVM registry chain
    /// shares the one 0x… address; the ELA mainchain gets its E… address. Called
    /// once on wallet first-load. Fire-and-forget (a failure just means a peer can't
    /// resolve us yet; it retries on the next wallet load).
    pub fn provision_tip_addresses(&self) {
        if self.tips_published {
            return;
        }
        let evm = self.state.wallet.evm_addr.clone();
        let ela = self.state.wallet.ela_addr.clone();
        let chains = self.state.wallet.chains.clone();
        if evm.is_empty() && ela.is_empty() {
            return; // addresses not resolved yet — provision on a later load
        }
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let mut addrs = serde_json::Map::new();
                // Every EVM registry chain (esc, ethereum, eid, …) shares the 0x address.
                if !evm.is_empty() {
                    for c in &chains {
                        if let Some(k) = c.get("key").and_then(Value::as_str) {
                            if !k.is_empty() {
                                addrs.insert(k.to_string(), Value::String(evm.clone()));
                            }
                        }
                    }
                }
                // The ELA mainchain receive address (E…).
                if !ela.is_empty() {
                    addrs.insert("ela".to_string(), Value::String(ela.clone()));
                }
                let json = Value::Object(addrs).to_string();
                hey_mobile_runtime::social::set_tip_addresses(&json).await
            },
            |r| match r {
                Ok(_) => UiEvent::Toast(String::new()),
                Err(e) => UiEvent::Error(format!("Couldn't publish tip address: {e}")),
            },
        );
    }

    /// Resolve a recipient's published receive addresses for the Tip sheet. Uses
    /// `refresh_contact_addresses` (== Android `refreshContact`): for a chat contact
    /// it first exchanges addresses over the DM channel so tipping resolves even
    /// without a follow, then falls back to the cached/feed lookup. NEVER guesses an
    /// address — if the recipient has published none, `addresses` comes back empty
    /// and the sheet refuses to send.
    pub fn resolve_tip(&self, did: &str) {
        let did = did.to_string();
        let key = did.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::refresh_contact_addresses(&did).await },
            move |addresses| UiEvent::TipResolved { did: key, addresses },
        );
    }

    /// Fetch the ERC-20 tokens the SENDER holds on ESC, for the tip asset picker
    /// (ESC is the only tippable EVM chain). Mirrors Android loading `balances("esc")`
    /// when the ESC chain is selected. Empty on any failure (native-only fallback).
    pub fn load_tip_tokens(&self, did: &str) {
        let did = did.to_string();
        self.engine.call(
            &self.ev_tx,
            || async { crate::walletops::balance("esc") },
            move |r| {
                let tokens = match r {
                    Ok(bal) => bal
                        .get("tokens")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                UiEvent::TipTokens { did, tokens }
            },
        );
    }

    /// Send a tip: sign + broadcast the transfer through the EXISTING wallet send
    /// path with `to_did` set (so walletops tags the record `kind:"tip"`), exactly
    /// like a normal Send but bound to the recipient's identity. The egui confirm
    /// screen is the spend gate. On success the recipient is notified over the DM
    /// channel (`notify_tip`). `to` MUST be an address resolved via `resolve_tip` —
    /// never a typed/guessed value (the Tip sheet only ever passes a resolved one).
    pub fn tip_send(&self, chain: String, token: Option<Value>, to: String, amount: String, to_did: String) {
        let notify_did = to_did.clone();
        let err_did = to_did.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let rec = crate::walletops::send(&chain, token.as_ref(), &to, &amount, &to_did)?;
                // Notify the recipient over the PRIVATE E2E DM channel that they were
                // tipped (same as Android's notifyTip) — best-effort, after the
                // on-chain transfer has landed, so a notify failure never blocks the
                // money path. Only reaches an established contact.
                let sym = rec.get("symbol").and_then(Value::as_str).unwrap_or("").to_string();
                let amt = rec.get("amount").and_then(Value::as_str).unwrap_or("").to_string();
                let hash = rec.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
                if !notify_did.is_empty() {
                    let _ = hey_mobile_runtime::social::notify_tip(&notify_did, &sym, &amt, &hash).await;
                }
                Ok::<Value, String>(rec)
            },
            move |r| match r {
                Ok(rec) => UiEvent::TipSent(rec),
                Err(e) => UiEvent::TipSendFailed { did: err_did, error: e },
            },
        );
    }

    /// Fetch the BIP39 recovery phrase for the backup sheet (the runtime resolves
    /// it from the in-process identity). Held only while the sheet is open.
    pub fn load_wallet_phrase(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::wallet_phrase() },
            |r| match r {
                Ok(p) => UiEvent::WalletPhrase(p),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// Poll an EVM tx's on-chain confirmation ONCE (the receipt) and emit the
    /// resulting status. The Done screen re-arms this on a "pending" result (capped
    /// in the handler) until it lands or the budget runs out — never blocking the UI.
    /// EVM only: the ELA mainchain has no receipt lookup, so the caller doesn't poll
    /// it (its Done stays broadcast-only). `hash` is echoed back so a stale poll for a
    /// closed/replaced send is ignored.
    pub fn poll_tx_status(&self, chain: &str, hash: &str) {
        let (chain, hash) = (chain.to_string(), hash.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move {
                // Brief settle before the first receipt read (mirrors Android's 3s
                // cadence) — the worker sleeps, never the UI thread.
                std::thread::sleep(std::time::Duration::from_secs(3));
                let status = crate::walletops::tx_status(&chain, &hash)
                    .unwrap_or_else(|_| "pending".to_string());
                (hash, status)
            },
            |(hash, status)| UiEvent::WalletTxStatus { hash, status },
        );
    }

    // ── hidden tokens (scam/dust protection — local persistence) ──────────────
    // The engine has no hidden-token fn, so this mirrors Android's SharedPreferences
    // `hidden_tokens` locally: one small JSON file next to the data dir (same
    // convention as theme.txt / chat-prefs.json / wallet-history.json). Keyed
    // "chainKey:contract". Every read is best-effort — a missing or garbled file
    // defaults to an empty set so a first run (or a corrupt file) never panics.

    fn hidden_tokens_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.engine.store).join("hidden-tokens.json")
    }

    /// Load the hidden-token set from disk into state (called once at boot).
    pub fn load_hidden_tokens(&mut self) {
        let Ok(s) = std::fs::read_to_string(self.hidden_tokens_path()) else { return };
        let Ok(v) = serde_json::from_str::<Value>(&s) else { return };
        self.state.wallet.hidden_tokens = v
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
    }

    fn save_hidden_tokens(&self) {
        let keys: Vec<&String> = self.state.wallet.hidden_tokens.iter().collect();
        if let Ok(s) = serde_json::to_string(&keys) {
            let _ = std::fs::write(self.hidden_tokens_path(), s);
        }
    }

    /// Hide / unhide one token (== Android `setTokenHidden`), persisting the change.
    pub fn set_token_hidden(&mut self, chain: &str, contract: &str, hidden: bool) {
        if contract.trim().is_empty() {
            return; // native coin has no contract — never hideable
        }
        let key = format!("{chain}:{contract}");
        if hidden {
            self.state.wallet.hidden_tokens.insert(key);
        } else {
            self.state.wallet.hidden_tokens.remove(&key);
        }
        self.save_hidden_tokens();
    }

    /// Path of the persisted local tx history (sent + tips).
    fn wallet_history_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.engine.store).join("wallet-history.json")
    }

    /// Load the persisted tx history into state (called once at boot).
    pub fn load_wallet_history(&mut self) {
        if let Ok(s) = std::fs::read_to_string(self.wallet_history_path()) {
            if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&s) {
                self.state.wallet.history = a;
            }
        }
    }

    fn save_wallet_history(&self) {
        if let Ok(s) = serde_json::to_string(&self.state.wallet.history) {
            let _ = std::fs::write(self.wallet_history_path(), s);
        }
    }

    // ── per-chat local prefs (mute / block) ───────────────────────────────────
    // Mirrors Android's SharedPreferences `muted_chats` / `blocked_dids`. Persisted
    // as one small JSON file next to the data dir (same convention as theme.txt and
    // wallet-history.json). Every read is best-effort: a missing or garbled file
    // defaults to empty sets so a first run (or a corrupt file) never panics.

    fn chat_prefs_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.engine.store).join("chat-prefs.json")
    }

    /// Load muted_chats / blocked_dids from disk into state (called once at boot).
    pub fn load_chat_prefs(&mut self) {
        let Ok(s) = std::fs::read_to_string(self.chat_prefs_path()) else { return };
        let Ok(v) = serde_json::from_str::<Value>(&s) else { return };
        let set = |v: &Value, key: &str| -> std::collections::HashSet<String> {
            v.get(key)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        self.state.muted_chats = set(&v, "muted_chats");
        self.state.blocked_dids = set(&v, "blocked_dids");
    }

    fn save_chat_prefs(&self) {
        let muted: Vec<&String> = self.state.muted_chats.iter().collect();
        let blocked: Vec<&String> = self.state.blocked_dids.iter().collect();
        let v = serde_json::json!({ "muted_chats": muted, "blocked_dids": blocked });
        if let Ok(s) = serde_json::to_string(&v) {
            let _ = std::fs::write(self.chat_prefs_path(), s);
        }
    }

    /// Toggle the muted flag for a chat id, persisting the change (== Android
    /// `setChatMuted`). Returns the new state so the caller can reflect it.
    pub fn set_chat_muted(&mut self, chat_id: &str, muted: bool) {
        if muted {
            self.state.muted_chats.insert(chat_id.to_string());
        } else {
            self.state.muted_chats.remove(chat_id);
        }
        self.save_chat_prefs();
    }

    /// Block & remove a DM contact (== Android: setBlocked(true) + deleteChat). The
    /// did is blocked locally (filtered from the chat list) and the conversation is
    /// deleted via the SAME engine path the delete-confirm uses.
    pub fn block_and_remove(&mut self, chat: &OpenChat) {
        self.state.blocked_dids.insert(chat.id.clone());
        self.save_chat_prefs();
        // ENGINE block (persisted) — without this the block was UI-only and a blocked peer could
        // still DM + ring you. Arm BOTH: set_blocked (the DM/call blocklist; is_blocked drops their
        // inbound DMs + call rings) AND block_follower (remove follower + disable chat + courtesy
        // signal). Mirrors Android's setBlocked. Persists, so no boot re-arm needed.
        let did = chat.id.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move {
                hey_core::api::dms::set_blocked(&did, true).await;
                let _ = hey_mobile_runtime::social::block_follower(&did).await;
                Ok::<(), String>(())
            },
            |r| match r {
                Ok(_) => UiEvent::Toast("Blocked".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
        self.delete_chat(chat);
        if self.state.open_chat.as_ref().map(|c| c.id == chat.id).unwrap_or(false) {
            self.state.open_chat = None;
            self.state.convo.clear();
        }
        self.load_chats();
    }

    /// Flush critical on-disk state, then relaunch this binary so the runtime loads
    /// the just-written identity (the live identity is fixed at boot). Guards against
    /// double-spawn / re-entry (§7 exit(0) relaunch race): the embedded runtime owns
    /// a fixed loopback port, so a second spawn while the old process is still bound
    /// would lose the bind race. We flush + best-effort fsync the data dir so the
    /// identity/seed/history can't be half-written when the new process starts.
    fn relaunch(&mut self) {
        if self.relaunching {
            return; // already on the way out — never spawn twice or re-exit
        }
        self.relaunching = true;
        // Flush wallet history + best-effort fsync the data dir so the identity write
        // (already completed on the worker before this event was emitted) and the
        // history are durable before the new instance races to read them.
        self.save_wallet_history();
        if let Ok(dir) = std::fs::File::open(&self.engine.store) {
            let _ = dir.sync_all();
        }
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).spawn();
        }
        std::process::exit(0);
    }

    pub fn load_chats(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::chat_contacts().await },
            |v| UiEvent::Contacts(as_array(v)),
        );
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::chat_groups().await },
            |v| UiEvent::Groups(as_array(v)),
        );
    }

    pub fn load_profile(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::get_profile("").await },
            UiEvent::Profile,
        );
    }

    pub fn load_activity(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::followers().await },
            |r| UiEvent::Followers(r.map(as_array).unwrap_or_default()),
        );
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::following().await },
            |r| UiEvent::Following(r.map(as_array).unwrap_or_default()),
        );
    }

    pub fn load_convo(&self, chat: &OpenChat) {
        let id = chat.id.clone();
        if chat.is_group {
            self.engine.call(
                &self.ev_tx,
                move || async move { hey_mobile_runtime::social::chat_group_conversation(&id).await },
                {
                    let cid = chat.id.clone();
                    move |v| UiEvent::Convo { id: cid, msgs: as_array(v) }
                },
            );
        } else {
            self.engine.call(
                &self.ev_tx,
                move || async move { hey_mobile_runtime::social::chat_conversation(&id).await },
                {
                    let cid = chat.id.clone();
                    move |v| UiEvent::Convo { id: cid, msgs: as_array(v) }
                },
            );
        }
    }

    pub fn load_unread(&self) {
        self.engine.call(
            &self.ev_tx,
            || async { hey_mobile_runtime::social::chat_unread().await },
            UiEvent::Unread,
        );
    }

    // ── post reactions / comments ─────────────────────────────────────────────

    pub fn load_reactions(&self, post_id: &str) {
        let (id, id2) = (post_id.to_string(), post_id.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::get_reactions(&id).await },
            move |r| UiEvent::Reactions {
                post_id: id2,
                summary: r.unwrap_or_else(|_| serde_json::json!({"counts":{},"mine":null,"total":0})),
            },
        );
    }

    pub fn load_comments(&self, post_id: &str) {
        let (id, id2) = (post_id.to_string(), post_id.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::get_comments(&id).await },
            move |r| UiEvent::Comments {
                post_id: id2,
                list: r.unwrap_or_else(|_| serde_json::json!([])),
            },
        );
    }

    pub fn react(&self, post_id: &str, emoji: &str) {
        let (id, e, id2) = (post_id.to_string(), emoji.to_string(), post_id.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::react(&id, &e).await },
            move |r| match r {
                Ok(s) => UiEvent::Reactions { post_id: id2, summary: s },
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn add_comment(&self, post_id: &str, text: &str, parent: &str) {
        let (id, t, p, id2) = (
            post_id.to_string(),
            text.to_string(),
            parent.to_string(),
            post_id.to_string(),
        );
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let _ = hey_mobile_runtime::social::add_comment(&id, &t, &p).await;
                hey_mobile_runtime::social::get_comments(&id).await
            },
            move |r| UiEvent::Comments {
                post_id: id2,
                list: r.unwrap_or_else(|_| serde_json::json!([])),
            },
        );
    }

    pub fn delete_post(&self, id: &str) {
        let pid = id.to_string();
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::delete_post(&pid).await },
            |r| match r {
                Ok(_) => UiEvent::FeedRevBumped,
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn edit_post(&self, id: &str, caption: &str) {
        let (pid, c) = (id.to_string(), caption.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::edit_post(&pid, &c).await },
            |r| match r {
                Ok(_) => UiEvent::FeedRevBumped,
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    // ── composer ──────────────────────────────────────────────────────────────

    pub fn create_post(&self, caption: String, tiles: Vec<Value>) {
        let tiles_json = serde_json::to_string(&tiles).unwrap_or_else(|_| "[]".into());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::create_post(&caption, &tiles_json).await },
            |r| match r {
                Ok(_) => UiEvent::Posted,
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    // ── follow / profile / chat-start ─────────────────────────────────────────

    pub fn follow(&self, input: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::follow(&input).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Followed".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// CHAT-ONLY pairing from a scanned/pasted link (hyper:chat: or a legacy friend link) — opens a
    /// 1:1 chat WITHOUT following. The engine enforces follow!=chat: a hyper:follow link routed here
    /// is rejected with a clear error (and vice-versa).
    pub fn chat_from_link(&self, input: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::chat_from_link(&input).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Chat ready".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// Fetch the 60-digit safety number for a contact (OOB MITM check), shown in the chat-info sheet.
    pub fn fetch_safety_number(&self, did: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let n = hey_mobile_runtime::social::safety_number(&did).await;
                Ok::<_, String>((did, n))
            },
            |r| match r {
                Ok((did, number)) => UiEvent::SafetyNumber { did, number },
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    /// Mark a contact's keys VERIFIED (the user compared the safety number out-of-band). Clears any
    /// key-changed alarm + the first-send gate. Mirrors Android's verify_contact.
    pub fn verify_contact(&self, did: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::verify_contact(&did).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Verified ✓".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn follow_back(&self, did: &str) {
        let did = did.to_string();
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::follow_back(&did).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Followed back".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn unfollow(&self, did: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::unfollow(&did).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Unfollowed".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn set_profile(&self, nick: String, bio: String, avatar: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::set_profile(&nick, &bio, &avatar).await },
            |r| match r {
                Ok(v) => UiEvent::Profile(v),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn start_chat(&self, did: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::start_chat(&did).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Chat ready".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    // ── chat attachments / message reactions / groups / invites ───────────────

    pub fn fetch_attachment(&self, key: String, att_json: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::chat_fetch_attachment(&att_json).await },
            move |r| UiEvent::AttachmentBytes {
                key,
                bytes: r.unwrap_or_default(),
            },
        );
    }

    pub fn react_message(&self, chat: &OpenChat, msg_id: String, emoji: String) {
        let (id, is_group, reload) = (chat.id.clone(), chat.is_group, chat.clone());
        self.engine.call(
            &self.ev_tx,
            move || async move {
                hey_mobile_runtime::social::chat_react_message(&id, &msg_id, &emoji, is_group).await
            },
            |r| match r {
                Ok(_) => UiEvent::Toast(String::new()),
                Err(e) => UiEvent::Error(e),
            },
        );
        self.load_msg_reactions(&reload);
    }

    /// Edit one of OUR own messages (DM or group). Shared engine fn — same one
    /// Android calls via hey_edit_message. Reloads the conversation on completion.
    pub fn edit_message(&self, chat: &OpenChat, msg_id: String, new_text: String) {
        let (id, is_group, reload) = (chat.id.clone(), chat.is_group, chat.clone());
        self.engine.call(
            &self.ev_tx,
            move || async move {
                hey_mobile_runtime::social::edit_chat_message(&id, &msg_id, &new_text, is_group).await
            },
            |ok: bool| if ok { UiEvent::Toast(String::new()) } else { UiEvent::Error("Couldn't edit message".into()) },
        );
        self.load_convo(&reload);
    }

    /// Delete one of OUR own messages for everyone (tombstone). Shared engine fn
    /// (hey_delete_message on Android). Reloads the conversation on completion.
    pub fn delete_message(&self, chat: &OpenChat, msg_id: String) {
        let (id, is_group, reload) = (chat.id.clone(), chat.is_group, chat.clone());
        self.engine.call(
            &self.ev_tx,
            move || async move {
                hey_mobile_runtime::social::delete_chat_message(&id, &msg_id, is_group).await
            },
            |ok: bool| if ok { UiEvent::Toast(String::new()) } else { UiEvent::Error("Couldn't delete message".into()) },
        );
        self.load_convo(&reload);
    }

    pub fn load_msg_reactions(&self, chat: &OpenChat) {
        let (id, is_group, cid) = (chat.id.clone(), chat.is_group, chat.id.clone());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::chat_message_reactions(&id, is_group).await },
            move |v| UiEvent::MsgReactions { chat_id: cid, list: v },
        );
    }

    pub fn create_group(&self, name: String, members: Vec<String>) {
        let members_json = serde_json::to_string(&members).unwrap_or_else(|_| "[]".into());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::chat_create_group(&name, &members_json).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Group created".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn accept_invite(&self, token: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::chat_accept_invite(&token).await },
            |r| match r {
                Ok(_) => UiEvent::Toast("Contact added".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    pub fn delete_chat(&self, chat: &OpenChat) {
        let (id, is_group) = (chat.id.clone(), chat.is_group);
        self.engine.call(
            &self.ev_tx,
            move || async move {
                if is_group {
                    hey_mobile_runtime::social::delete_group(&id).await
                } else {
                    hey_mobile_runtime::social::delete_conversation(&id).await
                }
            },
            |r| match r {
                Ok(_) => UiEvent::Toast("Deleted".into()),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    // ── full-screen peer profile overlay ──────────────────────────────────────

    pub fn load_user(&self, did: &str) {
        let (d1, d2, d3) = (did.to_string(), did.to_string(), did.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::get_profile(&d1).await },
            UiEvent::ViewedProfile,
        );
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::user_posts(&d2).await },
            |v| UiEvent::ViewedPosts(as_array(v)),
        );
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::is_following(&d3).await },
            |r| {
                UiEvent::ViewedFollowing(
                    r.ok()
                        .and_then(|v| v.get("following").and_then(Value::as_bool))
                        .unwrap_or(false),
                )
            },
        );
    }

    // ── file pickers (driven on an engine worker: pick → read → upload) ───────

    /// Composer multi-pick: open the portal file chooser, downscale/encode each
    /// image, upload, and append the resulting tiles to the composer.
    pub fn pick_media(&self) {
        self.engine.call(
            &self.ev_tx,
            || async {
                let mut tiles = Vec::new();
                if let Some(files) = rfd::AsyncFileDialog::new()
                    .add_filter(
                        "Media",
                        &["png", "jpg", "jpeg", "webp", "gif", "bmp", "mp4", "mov", "webm", "m4v"],
                    )
                    .pick_files()
                    .await
                {
                    for f in files.into_iter().take(10) {
                        let name = f.file_name();
                        let bytes = f.read().await;
                        let (data, mime) = crate::util::process_media(bytes, &name);
                        if let Ok(tile) = hey_mobile_runtime::social::upload_media(&data, &mime, &name).await {
                            tiles.push(tile);
                        }
                    }
                }
                tiles
            },
            UiEvent::MediaUploadedMany,
        );
    }

    /// Edit-profile / onboarding avatar pick: portal chooser → downscale → upload
    /// → set the resulting CID on the profile draft.
    pub fn pick_avatar(&self) {
        self.engine.call(
            &self.ev_tx,
            || async {
                if let Some(f) = rfd::AsyncFileDialog::new()
                    .add_filter("Image", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                    .pick_file()
                    .await
                {
                    let name = f.file_name();
                    let bytes = f.read().await;
                    let (data, mime) = crate::util::process_avatar(bytes);
                    if let Ok(tile) = hey_mobile_runtime::social::upload_media(&data, &mime, &name).await {
                        return tile
                            .get("cid")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("")
                            .to_string();
                    }
                }
                String::new()
            },
            UiEvent::PickedAvatarCid,
        );
    }

    /// Composer attach: multi-pick files, host-side scale/encode each (reusing the
    /// SAME `process_media` helper the feed + legacy path use), and STAGE them in the
    /// tray (no send yet) — the desktop parity for Android's `GetMultipleContents`
    /// picker. The cap is enforced on apply (in the `StagedPicked` handler), so the
    /// picker can return any number; surplus is dropped with a toast.
    pub fn pick_attachments(&mut self) {
        if self.state.staging_busy {
            return;
        }
        self.state.staging_busy = true;
        self.engine.call(
            &self.ev_tx,
            || async {
                let mut out: Vec<crate::state::StagedAttachment> = Vec::new();
                if let Some(files) = rfd::AsyncFileDialog::new().pick_files().await {
                    for f in files {
                        let name = f.file_name();
                        let bytes = f.read().await;
                        // Skip an empty/unreadable pick rather than staging a 0-byte
                        // item (never panic on a bad file — matches the constraint).
                        if bytes.is_empty() {
                            continue;
                        }
                        let (data, mime) = crate::util::process_media(bytes, &name);
                        out.push(crate::state::StagedAttachment { bytes: data, mime, name });
                    }
                }
                out
            },
            UiEvent::StagedPicked,
        );
    }

    /// Send every staged attachment (+ the optional caption), then clear the tray —
    /// the desktop parity for Android's `sendStaged`. Matching Android: the caption
    /// rides as its OWN leading text message (`chat_send`), then each attachment is
    /// sent with an EMPTY text via the SAME `chat_send_attachment` / group engine fn.
    /// A single engine job runs them sequentially (ordered) and emits a per-item
    /// `StagedProgress` so the composer can show "Sending d/t…"; the final
    /// `StagedSent` clears the tray + reloads the conversation.
    pub fn send_staged(&mut self, chat: &OpenChat, caption: String) {
        if self.state.sending || self.state.staged.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.state.staged);
        self.state.send_total = items.len();
        self.state.send_done = 0;
        self.state.sending = true;
        let (id, is_group, reload) = (chat.id.clone(), chat.is_group, chat.clone());
        let tx = self.ev_tx.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let caption = caption.trim().to_string();
                if !caption.is_empty() {
                    let _ = if is_group {
                        hey_mobile_runtime::social::chat_send_group(&id, &caption).await
                    } else {
                        hey_mobile_runtime::social::chat_send(&id, &caption).await
                    };
                }
                let mut done = 0usize;
                for it in items {
                    let _ = if is_group {
                        hey_mobile_runtime::social::chat_send_group_attachment(
                            &id, "", &it.bytes, &it.mime, &it.name,
                        )
                        .await
                    } else {
                        hey_mobile_runtime::social::chat_send_attachment(
                            &id, "", &it.bytes, &it.mime, &it.name,
                        )
                        .await
                    };
                    // A failed item is skipped (never aborts the batch); progress
                    // still advances so the bar can't wedge.
                    done += 1;
                    let _ = tx.send(UiEvent::StagedProgress(done));
                }
                reload
            },
            UiEvent::StagedSent,
        );
    }

    pub fn toast(&mut self, msg: impl Into<String>, now: f64) {
        self.state.toast = Some((msg.into(), now + 3.0));
    }

    /// Save already-decoded attachment bytes to disk (the desktop parity for
    /// Android's save-to-Photos). Opens the native save dialog on a worker (same
    /// idiom as `pick_and_send_attachment`); if the user cancels OR the dialog can't
    /// open, falls back to the OS Pictures (then Downloads) dir under a `Hey`
    /// subfolder. Toasts the written path — or the failure — either way. The bytes
    /// are the SAME ones already rendered inline (no re-fetch).
    pub fn save_attachment(&self, bytes: Vec<u8>, name: String) {
        self.engine.call(
            &self.ev_tx,
            move || async move {
                // Sanitise the suggested name; default to a timestamped one.
                let base: String = {
                    let n = if name.trim().is_empty() { "hey-photo".to_string() } else { name.clone() };
                    n.chars()
                        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
                        .collect()
                };
                if let Some(f) = rfd::AsyncFileDialog::new().set_file_name(&base).save_file().await {
                    return match f.write(&bytes).await {
                        Ok(_) => Ok(f.path().display().to_string()),
                        Err(e) => Err(format!("Couldn't save: {e}")),
                    };
                }
                // Dialog cancelled / unavailable → fall back to Pictures/Hey (then Downloads/Hey).
                let dir = dirs::picture_dir()
                    .or_else(dirs::download_dir)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("Hey");
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    return Err(format!("Couldn't save: {e}"));
                }
                let path = dir.join(&base);
                match std::fs::write(&path, &bytes) {
                    Ok(_) => Ok(path.display().to_string()),
                    Err(e) => Err(format!("Couldn't save: {e}")),
                }
            },
            |r| match r {
                Ok(path) => UiEvent::Toast(format!("Saved to {path}")),
                Err(e) => UiEvent::Error(e),
            },
        );
    }

    // ── event pump ────────────────────────────────────────────────────────────

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                UiEvent::Whoami { did, .. } => {
                    self.state.me_did = did;
                    self.state.booted = true;
                }
                UiEvent::FriendLink(s) => self.state.friend_link = s,
                UiEvent::FollowLink(s) => self.state.follow_link = s,
                UiEvent::ChatLink(s) => self.state.chat_link = s,
                UiEvent::CanChat { did, ok } => {
                    // Cache chat-capability for the OPEN chat (composer/call gate) AND in the global
                    // chatable set (drives the New-chat "people you follow" filter). The engine
                    // enforces sends regardless; this is the UI parity with Android.
                    if self.state.open_chat.as_ref().map(|c| c.id.as_str()) == Some(did.as_str()) {
                        self.state.open_chat_can_chat = ok;
                    }
                    if ok {
                        self.state.chatable_dids.insert(did);
                    } else {
                        self.state.chatable_dids.remove(&did);
                    }
                }
                UiEvent::SafetyNumber { did, number } => {
                    self.state.safety_did = did;
                    self.state.safety_number = number;
                }
                UiEvent::Health {
                    online, direct, direct_peers, relay_peers, peers,
                    public_v4, public_v6, ipv4, ipv6_global, udp_v4, udp_v6, local_addrs,
                } => {
                    self.state.online = online;
                    self.state.direct = direct;
                    self.state.direct_peers = direct_peers;
                    self.state.relay_peers = relay_peers;
                    self.state.peers = peers;
                    self.state.public_v4 = public_v4;
                    self.state.public_v6 = public_v6;
                    self.state.ipv4 = ipv4;
                    self.state.ipv6_global = ipv6_global;
                    self.state.udp_v4 = udp_v4;
                    self.state.udp_v6 = udp_v6;
                    self.state.local_addrs = local_addrs;
                }
                UiEvent::Feed(v) => {
                    self.state.feed = v;
                    self.state.feed_loaded = true;
                }
                UiEvent::Reactions { post_id, summary } => {
                    self.state.reactions.insert(post_id, summary);
                }
                UiEvent::Comments { post_id, list } => {
                    self.state.comments.insert(post_id, list);
                }
                UiEvent::Contacts(v) => {
                    self.state.contacts = v;
                    self.state.chats_loaded = true;
                }
                UiEvent::Groups(v) => self.state.groups = v,
                UiEvent::Convo { id, msgs } => {
                    if self.state.open_chat.as_ref().map(|c| c.id == id).unwrap_or(false) {
                        self.state.convo = msgs;
                    }
                }
                UiEvent::MsgReactions { chat_id, list } => {
                    self.state.msg_reactions.insert(chat_id, list);
                }
                UiEvent::AttachmentBytes { key, bytes } => {
                    self.state.att_loading.remove(&key);
                    self.state.put_attachment(key, bytes);
                }
                UiEvent::MediaUploadedMany(tiles) => {
                    for t in tiles {
                        if t.get("error").is_none() {
                            self.state.composer.tiles.push(t);
                        }
                    }
                    self.state.composer.busy = false;
                    self.state.composer.status =
                        format!("{} selected", self.state.composer.tiles.len());
                }
                UiEvent::PickedAvatarCid(cid) => {
                    if !cid.is_empty() {
                        self.state.profile_draft.avatar_cid = cid;
                    }
                    self.state.profile_draft.busy = false;
                }
                UiEvent::StagedPicked(items) => {
                    self.state.staging_busy = false;
                    if !items.is_empty() {
                        let room = crate::state::STAGED_CAP.saturating_sub(self.state.staged.len());
                        let dropped = items.len().saturating_sub(room);
                        self.state.staged.extend(items.into_iter().take(room));
                        if dropped > 0 {
                            let now = ctx.input(|i| i.time);
                            self.toast(
                                format!("Up to {} attachments — {dropped} skipped", crate::state::STAGED_CAP),
                                now,
                            );
                        }
                    }
                }
                UiEvent::StagedProgress(done) => {
                    self.state.send_done = done;
                }
                UiEvent::StagedSent(chat) => {
                    self.state.sending = false;
                    self.state.send_total = 0;
                    self.state.send_done = 0;
                    // Reload only if this is still the open conversation.
                    if self.state.open_chat.as_ref().map(|c| c.id == chat.id).unwrap_or(false) {
                        self.load_convo(&chat);
                    }
                }
                UiEvent::ViewedProfile(v) => {
                    if let Some(vu) = self.state.viewed.as_mut() {
                        vu.profile = v;
                        vu.loaded = true;
                    }
                }
                UiEvent::ViewedPosts(p) => {
                    if let Some(vu) = self.state.viewed.as_mut() {
                        vu.posts = p;
                    }
                }
                UiEvent::ViewedFollowing(b) => {
                    if let Some(vu) = self.state.viewed.as_mut() {
                        vu.following_them = b;
                    }
                }
                UiEvent::Unread(n) => {
                    // In-app surfacing (GAP 2): the rail Chat badge + bell read this.
                    self.state.unread = n;
                    // OS notification on a positive aggregate-unread DELTA (== Android
                    // RuntimeService: `if (u > lastUnread && u > 0) notifyEvent(...)`).
                    // The FIRST poll only seeds the baseline so a startup backlog of
                    // already-unread chats doesn't burst a notification (no-startup-burst).
                    // `chat_unread()` is aggregate (no per-chat breakdown), so mute is
                    // applied at the per-event layer (the Notif handler) where a sender
                    // did is available; this aggregate delta mirrors Android 1:1.
                    if self.state.notif_seeded {
                        if n > self.state.last_unread && n > 0 {
                            let body = if n == 1 {
                                "1 unread message".to_string()
                            } else {
                                format!("{n} unread messages")
                            };
                            crate::notify::post("New messages", &body);
                        }
                    } else {
                        self.state.notif_seeded = true;
                    }
                    self.state.last_unread = n;
                }
                UiEvent::Followers(v) => {
                    self.state.followers = v;
                    self.state.activity_loaded = true;
                }
                UiEvent::Following(v) => self.state.following = v,
                UiEvent::Profile(v) => self.state.profile = v,
                UiEvent::Notif(n) => {
                    // Per-event OS notification (mention / tip / post / follow) — the
                    // desktop parity for Android's `for (n in drainNotifs()) notifyEvent`.
                    // Each item already carries title/body/did from the engine. RESPECT
                    // MUTE: skip if the source `did` is a muted chat (Android doesn't
                    // filter here, but the desktop has the muted_chats set on hand and
                    // the task asks us to honour it). Always store it for the in-app bell.
                    let did = n.get("did").and_then(Value::as_str).unwrap_or("");
                    let muted = !did.is_empty() && self.state.muted_chats.contains(did);
                    if !muted {
                        let title = n.get("title").and_then(Value::as_str).unwrap_or("");
                        let title = if title.is_empty() { "Hey" } else { title };
                        let body = n.get("body").and_then(Value::as_str).unwrap_or("");
                        crate::notify::post(title, body);
                    }
                    if self.state.notifs.len() >= 50 {
                        self.state.notifs.pop_front();
                    }
                    self.state.notifs.push_back(n);
                }
                UiEvent::FeedRevBumped => self.load_feed(),
                UiEvent::Media { cid, img } => self.media.apply(ctx, cid, img),
                UiEvent::Posted => {
                    self.state.modal = None;
                    self.state.composer = Default::default();
                    self.load_feed();
                }
                UiEvent::WalletAddresses { evm, ela, did, chains } => {
                    let w = &mut self.state.wallet;
                    w.evm_addr = evm;
                    w.ela_addr = ela;
                    w.did = did;
                    w.chains = as_array(chains);
                    if w.chain.is_empty() {
                        w.chain = "esc".to_string();
                    }
                    w.loaded = true;
                    // Kick off a balance fetch for the selected chain.
                    let chain = w.chain.clone();
                    w.refreshing.insert(chain.clone());
                    self.load_wallet_balance(&chain);
                    // Publish MY receive addresses once (== Android publishTipAddresses
                    // on wallet load) so others can tip me by identity.
                    if !self.tips_published {
                        self.tips_published = true;
                        self.provision_tip_addresses();
                    }
                }
                UiEvent::WalletBalance { chain, data } => {
                    self.state.wallet.refreshing.remove(&chain);
                    if let Some(e) = data.get("error").and_then(Value::as_str) {
                        let now = ctx.input(|i| i.time);
                        self.state.toast = Some((e.to_string(), now + 4.0));
                    } else {
                        self.state.wallet.balances.insert(chain, data);
                    }
                }
                UiEvent::WalletSent(rec) => {
                    let hash = rec.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
                    let chain = rec.get("chain").and_then(Value::as_str).unwrap_or("").to_string();
                    self.state.wallet.history.insert(0, rec);
                    if self.state.wallet.history.len() > 200 {
                        self.state.wallet.history.truncate(200);
                    }
                    self.save_wallet_history();
                    // Advance the send sheet to its Done state + refresh balance. For an
                    // EVM send the hash only means the node ACCEPTED it — start a receipt
                    // poll so the Done screen flips Pending → Confirmed/Failed. The ELA
                    // mainchain has no receipt lookup, so its Done is left broadcast-only.
                    let is_evm = !chain.is_empty() && chain != "ela";
                    let s = &mut self.state.wallet.send;
                    s.stage = crate::state::SendStage::Done;
                    s.status = String::new();
                    s.tx_hash = hash.clone();
                    s.conf = "pending".to_string();
                    s.polling = is_evm && !hash.is_empty();
                    if !chain.is_empty() {
                        self.state.wallet.refreshing.insert(chain.clone());
                        self.load_wallet_balance(&chain);
                    }
                    if is_evm && !hash.is_empty() {
                        // Fresh poll budget for this send.
                        ctx.data_mut(|d| d.insert_temp(egui::Id::new("wallet-poll-n"), 0u32));
                        self.poll_tx_status(&chain, &hash);
                    }
                    let now = ctx.input(|i| i.time);
                    self.state.toast = Some(("Transaction broadcast".into(), now + 3.0));
                }
                UiEvent::WalletTxStatus { hash, status } => {
                    // Ignore a stale poll (the sheet closed, or a newer send replaced
                    // the hash). The poll budget is bounded by re-arm count, capped here.
                    let s = &mut self.state.wallet.send;
                    if s.open && s.tx_hash == hash && s.stage == crate::state::SendStage::Done {
                        if status == "success" || status == "failed" {
                            s.conf = status;
                            s.polling = false;
                        } else {
                            // Still pending → re-arm, up to ~24 polls (~72s) like Android,
                            // then give up gracefully (stops; Done stays "Broadcast").
                            let n = ctx
                                .data(|d| d.get_temp::<u32>(egui::Id::new("wallet-poll-n")))
                                .unwrap_or(0);
                            if n < 24 {
                                ctx.data_mut(|d| d.insert_temp(egui::Id::new("wallet-poll-n"), n + 1));
                                let chain = self
                                    .state
                                    .wallet
                                    .history
                                    .first()
                                    .and_then(|r| r.get("chain").and_then(Value::as_str))
                                    .unwrap_or("esc")
                                    .to_string();
                                self.poll_tx_status(&chain, &hash);
                            } else {
                                s.polling = false;
                            }
                        }
                    }
                }
                UiEvent::WalletSendFailed(e) => {
                    let s = &mut self.state.wallet.send;
                    s.stage = crate::state::SendStage::Review; // back to confirm so they can retry
                    s.status = e.clone();
                    let now = ctx.input(|i| i.time);
                    self.state.toast = Some((e, now + 5.0));
                }
                UiEvent::TipResolved { did, addresses } => {
                    // Ignore a stale resolve if the sheet closed or retargeted.
                    if self.state.tip.open && self.state.tip.did == did {
                        // chainKey -> address
                        let mut map = std::collections::HashMap::new();
                        if let Some(obj) = addresses.as_object() {
                            for (k, v) in obj {
                                if let Some(s) = v.as_str() {
                                    if !s.trim().is_empty() {
                                        map.insert(k.clone(), s.to_string());
                                    }
                                }
                            }
                        }
                        // Tippable chains, in display order (mirrors Android): the ELA
                        // main chain, then ESC if the recipient published an esc address
                        // AND the sender's wallet carries that chain. (EID is identity
                        // plumbing, not money; long-tail EVM chains stay in the wallet.)
                        let mut chains: Vec<(String, String)> = Vec::new();
                        if map.contains_key("ela") {
                            chains.push(("ela".to_string(), "ELA".to_string()));
                        }
                        let has_esc = self
                            .state
                            .wallet
                            .chains
                            .iter()
                            .any(|c| c.get("key").and_then(Value::as_str) == Some("esc"));
                        if has_esc && map.contains_key("esc") {
                            chains.push(("esc".to_string(), "ELA".to_string())); // ESC native is ELA
                        }
                        let t = &mut self.state.tip;
                        t.addresses = map;
                        t.chain = chains.first().map(|(k, _)| k.clone()).unwrap_or_default();
                        t.symbol = chains.first().map(|(_, s)| s.clone()).unwrap_or_default();
                        t.chains = chains;
                        t.token = None;
                        t.tokens.clear();
                        t.stage = crate::state::TipStage::Edit;
                        // If ESC is selected, load the sender's ERC-20s for the asset picker.
                        if t.chain == "esc" {
                            self.load_tip_tokens(&did);
                        }
                    }
                }
                UiEvent::TipTokens { did, tokens } => {
                    if self.state.tip.open && self.state.tip.did == did && self.state.tip.chain == "esc" {
                        self.state.tip.tokens = tokens;
                    }
                }
                UiEvent::TipSent(rec) => {
                    let hash = rec.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
                    let sym = rec.get("symbol").and_then(Value::as_str).unwrap_or("").to_string();
                    let chain = rec.get("chain").and_then(Value::as_str).unwrap_or("").to_string();
                    // A tip is a spend → it also belongs in the wallet history (kind:"tip").
                    self.state.wallet.history.insert(0, rec);
                    if self.state.wallet.history.len() > 200 {
                        self.state.wallet.history.truncate(200);
                    }
                    self.save_wallet_history();
                    let t = &mut self.state.tip;
                    t.stage = crate::state::TipStage::Done;
                    t.status = String::new();
                    t.tx_hash = hash;
                    if !sym.is_empty() {
                        t.symbol = sym;
                    }
                    // Refresh the spent chain's balance.
                    if !chain.is_empty() {
                        self.state.wallet.refreshing.insert(chain.clone());
                        self.load_wallet_balance(&chain);
                    }
                    let now = ctx.input(|i| i.time);
                    self.state.toast = Some(("Tip sent".into(), now + 3.0));
                }
                UiEvent::TipSendFailed { did, error } => {
                    if self.state.tip.open && self.state.tip.did == did {
                        let t = &mut self.state.tip;
                        t.stage = crate::state::TipStage::Review; // back to confirm so they can retry
                        t.status = error.clone();
                    }
                    let now = ctx.input(|i| i.time);
                    self.state.toast = Some((error, now + 5.0));
                }
                UiEvent::WalletPhrase(p) => self.state.wallet.phrase = Some(p),
                UiEvent::WalletLocked => self.state.wallet.locked = true,
                UiEvent::WalletSeedCreated => {
                    // The live identity is fixed at boot — flush, then relaunch to load
                    // the new BIP39 seed (guarded against double-spawn). See §7.
                    self.relaunch();
                }
                UiEvent::OnboardRestored => {
                    // Mark onboarded, flush, then relaunch so the restored identity
                    // loads. The .hey-onboarded marker must be on disk before exit.
                    let _ = std::fs::write(
                        std::path::Path::new(&self.engine.store).join(".hey-onboarded"),
                        b"1",
                    );
                    self.relaunch();
                }
                UiEvent::OnboardError(e) => {
                    self.state.onboarding.busy = false;
                    self.state.onboarding.error = e;
                }
                UiEvent::OnboardProfileSet(r) => {
                    // CREATE-new profile saved (or failed) → store it locally + enter
                    // the app regardless. A failure only toasts; the user is never
                    // trapped on the setup screen (matches Android's runCatching).
                    self.state.profile_draft.busy = false;
                    match r {
                        Ok(v) => self.state.profile = v,
                        Err(e) => {
                            log::warn!("onboarding set_profile failed: {e}");
                            let now = ctx.input(|i| i.time);
                            self.state.toast =
                                Some(("Couldn't save your profile — you can edit it later".into(), now + 4.0));
                        }
                    }
                    self.finish_onboarding();
                }
                UiEvent::CallSignals(sigs) => {
                    for s in sigs {
                        let from = s.get("from").and_then(Value::as_str).unwrap_or("").to_string();
                        let p = s.get("payload").cloned().unwrap_or(Value::Null);
                        let ty = p.get("type").and_then(Value::as_str).unwrap_or("").to_string();
                        let call_id = p.get("call_id").and_then(Value::as_str).unwrap_or("").to_string();
                        let video = p.get("video").and_then(Value::as_bool).unwrap_or(false);
                        if from.is_empty() || call_id.is_empty() {
                            continue;
                        }
                        let name = p
                            .get("name")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| AppState::short_did(&from));
                        match ty.as_str() {
                            "offer" => {
                                // Ghost-ring: ignore a just-ended id lingering in the 2-min window.
                                if self.state.last_ended_call_id.as_deref() == Some(call_id.as_str()) {
                                    continue;
                                }
                                if !matches!(self.state.call, CallState::Idle) {
                                    // Busy → auto-decline so the caller stops ringing.
                                    self.send_call_sig(&from, "decline", &call_id, false);
                                    continue;
                                }
                                self.state.call = CallState::Incoming { peer: from, name, call_id, video };
                                self.start_ring(ctx);
                                ctx.request_repaint();
                            }
                            "accept" => {
                                if let CallState::Outgoing { peer, name, call_id: cid, video: ours } =
                                    self.state.call.clone()
                                {
                                    if cid == call_id && peer == from {
                                        let video = ours || video;
                                        self.state.call = CallState::Active {
                                            peer, name, call_id: cid, video, is_caller: true,
                                        };
                                        self.call_since = Some(std::time::Instant::now());
                                        self.start_media();
                                        ctx.request_repaint();
                                    }
                                }
                            }
                            "decline" | "end" => {
                                if self.state.call.call_id() == Some(call_id.as_str()) {
                                    self.end_local(&call_id);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                UiEvent::ContactTransport { did, transport } => {
                    self.state.call_transport.insert(did.clone(), transport.clone());
                    // Mid-call demote: a live VIDEO call whose path flapped to relay → voice.
                    if transport == "relay" {
                        if let CallState::Active { peer, name, call_id, video: true, is_caller } =
                            self.state.call.clone()
                        {
                            if peer == did {
                                hey_mobile_runtime::video_stop();
                                self.state.call = CallState::Active {
                                    peer, name, call_id, video: false, is_caller,
                                };
                                let now = ctx.input(|i| i.time);
                                self.state.toast =
                                    Some(("Switched to voice (relay path)".into(), now + 3.0));
                            }
                        }
                    }
                }
                UiEvent::Toast(m) => {
                    if !m.is_empty() {
                        let now = ctx.input(|i| i.time);
                        self.state.toast = Some((m, now + 3.0));
                    }
                }
                UiEvent::Error(e) => {
                    log::warn!("engine error: {e}");
                    // Never leave a spinner stuck on a failed op.
                    self.state.composer.busy = false;
                    self.state.profile_draft.busy = false;
                    self.state.sheets.add_busy = false;
                    self.state.sheets.group_busy = false;
                    self.state.composer.status = e.clone();
                    let now = ctx.input(|i| i.time);
                    self.state.toast = Some((e, now + 4.0));
                }
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now >= self.next_health {
            self.load_health();
            self.next_health = now + 3.0;
        }
        if now >= self.next_chats {
            self.load_chats();
            self.load_unread();
            // The friend link/QR needs the carrier ticket, which isn't ready at
            // boot — retry until it resolves (then the invite QR can render).
            if self.state.friend_link.is_empty() {
                self.load_friend_link();
            }
            self.next_chats = now + 2.0;
        }
        if (self.state.tab == Tab::Activity || self.state.show_notifs) && now >= self.next_activity {
            self.load_activity();
            self.next_activity = now + 3.0;
        }
        if self.state.open_chat.is_some() && now >= self.next_convo {
            if let Some(c) = self.state.open_chat.clone() {
                self.load_convo(&c);
                self.load_msg_reactions(&c);
            }
            self.next_convo = now + 1.5;
        }
        // Calls: poll inbound signals (offer/accept/decline/end) every 1s. While in
        // a VIDEO call also re-check the peer transport so we can demote to voice if
        // the path flaps to relay (iroh multipath churn — a start-time check isn't durable).
        if now >= self.next_call_poll {
            self.poll_call_signals();
            if let CallState::Active { peer, video: true, .. } = &self.state.call {
                self.probe_transport(peer.clone());
            }
            self.next_call_poll = now + 1.0;
        }
        if let Some((_, exp)) = self.state.toast {
            if now >= exp {
                self.state.toast = None;
            }
        }
    }

    // ── chrome ────────────────────────────────────────────────────────────────

    /// Platform-correct modifier label for tooltips ("⌘" on macOS, "Ctrl" else).
    fn cmd() -> &'static str {
        if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" }
    }

    /// Toggle the Command Palette (Ctrl/Cmd+K). Opening seeds a fresh state with
    /// `just_opened` so the field grabs keyboard focus on its first frame.
    pub fn toggle_palette(&mut self) {
        self.state.palette = if self.state.palette.is_some() {
            None
        } else {
            Some(crate::state::PaletteState { just_opened: true, ..Default::default() })
        };
    }

    /// Apply a command the palette resolved (Enter / click). The palette closed
    /// itself as it returned this, so we only dispatch the effect against `&mut self`
    /// — the SAME state the global keymap drives.
    fn apply_palette_action(&mut self, ctx: &egui::Context, action: crate::views::palette::PaletteAction) {
        use crate::views::palette::PaletteAction as A;
        match action {
            A::Go(tab) => self.set_tab(ctx, tab),
            A::ToggleTheme => {
                self.state.light = !self.state.light;
                crate::theme::save_pref(self.state.light);
            }
            A::Settings => self.state.modal = Some(crate::state::Modal::Settings),
            A::NewPost => self.state.modal = Some(Modal::Composer),
            A::Connection => self.state.modal = Some(crate::state::Modal::Connection),
            A::CheatSheet => self.state.cheat_sheet = true,
            A::StartCall { video } => {
                if let Some(chat) = self.state.open_chat.clone() {
                    if !chat.is_group {
                        self.start_call(chat.id, chat.name, video);
                    }
                }
            }
            A::Stub(label) => {
                let now = ctx.input(|i| i.time);
                self.toast(format!("{label} — coming soon"), now);
            }
        }
    }

    /// The 56px icon SPINE — the flagship desktop chassis edge. Top to bottom:
    /// identity dot (encodes the live connection color) · Chat · Feed · Wallet ·
    /// Verse · Calls · (spacer) · You-avatar (self gold-ring + call pulse) · Settings.
    /// Icon-only, gold left tick + filled glyph on active, tooltip flyout naming the
    /// section + its shortcut. The labelled 240px rail is retired.
    fn rail(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        use crate::icons;
        // ── identity dot (top) — encodes the connection color, click → explainer ──
        let online = self.state.online;
        let peers = self.state.peers;
        let dp = self.state.direct_peers;
        let plural = if peers == 1 { "" } else { "s" };
        // Reflect REALITY: counts come from iroh's live per-peer paths. With no peers
        // there's no connection to be "direct" — say so honestly. (Logic relocated
        // verbatim from the old rail footer; surfaced here as a dot + status strip.)
        let (dot, _icon, label, sub) = if !online {
            (theme.gold_ink, icons::SWAP_HORIZ, "Connecting…", String::new())
        } else if peers == 0 {
            (theme.muted, icons::PUBLIC, "No peers", "online · 0 connected".to_string())
        } else if dp >= peers {
            (theme.good, icons::BOLT, "Direct", format!("{peers} peer{plural} · peer-to-peer"))
        } else if dp > 0 {
            (theme.good, icons::BOLT, "Mostly direct", format!("{dp}/{peers} peers direct"))
        } else {
            (theme.gold_ink, icons::HUB, "Relay-assisted", format!("{peers} peer{plural} · via relay"))
        };
        let tip = if !online {
            "Connecting to the Hey carrier…".to_string()
        } else {
            format!("{label}{}{}", if sub.is_empty() { "" } else { " — " }, sub)
        };
        ui.add_space(2.0);
        ui.vertical_centered(|ui| {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
            // The Hey mark behind the live connection dot: a small gold disc + a
            // connection-colored ring so the spine top reads as "you, and your link".
            ui.painter().circle_filled(r.center(), 8.0, GOLD);
            ui.painter().circle_stroke(r.center(), 11.0, egui::Stroke::new(2.0, dot));
            let resp = resp.on_hover_text(tip);
            if resp.clicked() {
                self.state.modal = Some(crate::state::Modal::Connection);
            }
        });
        ui.add_space(14.0);

        // ── sections (icon-only spine items, gold tick + tooltip flyout) ──────────
        self.rail_item(ui, theme, Tab::Chat, icons::FORUM, "Chat", "1", self.state.unread);
        self.rail_item(ui, theme, Tab::Feed, icons::DYNAMIC_FEED, "Feed", "2", 0);
        self.rail_item(ui, theme, Tab::Wallet, icons::ACCOUNT_BALANCE_WALLET, "Wallet", "3", 0);
        self.rail_item(ui, theme, Tab::Verse, icons::PUBLIC, "Verse", "4", 0);
        self.rail_item(ui, theme, Tab::Calls, icons::CALL, "Calls", "5", 0);

        // ── spacer → You-avatar + Settings pinned to the bottom ───────────────────
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            // Settings (gear) — the bottom-most spine affordance, Ctrl/Cmd+, .
            ui.add_space(2.0);
            {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect.shrink2(egui::vec2(8.0, 6.0)), 8.0, theme.hover);
                }
                let col = if resp.hovered() { theme.ink } else { theme.muted };
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    icons::SETTINGS,
                    FontId::proportional(20.0),
                    col,
                );
                let resp = resp.on_hover_text(format!("Settings  {},", Self::cmd()));
                if resp.clicked() {
                    self.state.modal = Some(crate::state::Modal::Settings);
                }
            }
            ui.add_space(6.0);
            // You / self-avatar — the self gold-ring (the ONLY gold ring) + a live-call
            // pulse ring when a call is in progress. Click → the You section (slot 6).
            {
                let in_call = !matches!(self.state.call, CallState::Idle);
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::click());
                let selected = self.state.tab == Tab::Profile;
                if selected {
                    self.paint_spine_tick(ui, rect);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect.shrink2(egui::vec2(7.0, 5.0)), 8.0, theme.hover);
                }
                // Draw my own avatar (avatar() paints the gold self-ring via "me-did").
                let av = 30.0;
                let avrect = egui::Rect::from_center_size(rect.center(), egui::vec2(av, av));
                let mut child = ui.child_ui(avrect, egui::Layout::top_down(egui::Align::Center), None);
                let avatar_cid = self
                    .state
                    .profile
                    .get("avatar")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                crate::views::avatar(
                    &mut self.media, &self.engine, &self.ev_tx, &mut child,
                    &avatar_cid, &self.state.me_did, av,
                );
                // Live-call pulse ring (a breathing gold halo) while a call is active.
                if in_call {
                    let t = ui.ctx().input(|i| i.time);
                    let pulse = 0.5 + 0.5 * ((t * 2.4).sin() as f32);
                    ui.painter().circle_stroke(
                        rect.center(),
                        av / 2.0 + 3.0,
                        egui::Stroke::new(2.0, GOLD.gamma_multiply(0.35 + 0.45 * pulse)),
                    );
                    ui.ctx().request_repaint();
                }
                let resp = resp.on_hover_text(format!("You  {}6", Self::cmd()));
                if resp.clicked() {
                    self.set_tab(ui.ctx(), Tab::Profile);
                }
            }
        });
    }

    /// Paint the spine/list-row "this is current" marker: a 2px full-height gold
    /// LEFT TICK + a faint 0.10 wash. The single selection signal the eye learns.
    fn paint_spine_tick(&self, ui: &egui::Ui, rect: egui::Rect) {
        let p = ui.painter();
        let theme = Theme::get(self.state.light);
        // 0.10 wash under the item.
        p.rect_filled(rect.shrink2(egui::vec2(6.0, 4.0)), 8.0, GOLD.gamma_multiply(0.10));
        // 2px full-height gold tick hugging the spine's left edge.
        let tick = egui::Rect::from_min_max(
            egui::pos2(rect.left() - 8.0, rect.top() + 3.0),
            egui::pos2(rect.left() - 6.0, rect.bottom() - 3.0),
        );
        p.rect_filled(tick, 1.0, theme.gold_tick);
    }

    /// A single ICON-ONLY spine item (56px-wide column): a centered 21px glyph, the
    /// gold-pill animate_bool + press-spring KEPT, plus the new flagship cues — a 2px
    /// full-height gold LEFT TICK on the active item, the active glyph → gold_ink, a
    /// neutral hover wash, the existing LIKE unread pill top-right, and a tooltip
    /// flyout naming the section + its keyboard shortcut ("Chat  ⌘1").
    fn rail_item(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        tab: Tab,
        icon: &str,
        label: &str,
        shortcut: &str,
        badge: u32,
    ) {
        let selected = self.state.tab == tab;
        let t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::new(("rail", tab as u8)), selected, 0.10);
        let h = 44.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());
        // Press spring: shrink the glyph wash slightly while the pointer is held.
        let press = ui.ctx().animate_bool_with_time(
            egui::Id::new(("railp", tab as u8)),
            resp.is_pointer_button_down_on(),
            0.06,
        );
        // The clickable cell is the full column width; the painted pill/wash sits in
        // an inset square so the gold tick can hug the spine's left edge outside it.
        let pill = rect.shrink2(egui::vec2(6.0, 4.0)).shrink(1.0 * press);
        let p = ui.painter();
        if selected {
            // 2px full-height gold LEFT TICK + 0.10 wash (animated in via `t`).
            p.rect_filled(pill, 8.0, GOLD.gamma_multiply(0.18 * t));
            let tick = egui::Rect::from_min_max(
                egui::pos2(rect.left() - 8.0, rect.top() + 3.0),
                egui::pos2(rect.left() - 6.0, rect.bottom() - 3.0),
            );
            p.rect_filled(tick, 1.0, theme.gold_tick);
        } else if t > 0.001 {
            p.rect_filled(pill, 8.0, GOLD.gamma_multiply(0.18 * t));
        } else if resp.hovered() {
            p.rect_filled(pill, 8.0, theme.hover);
        }
        // Active glyph → gold_ink; hover → ink; rest → muted.
        let icon_col = if selected {
            theme.gold_ink
        } else if resp.hovered() {
            theme.ink
        } else {
            theme.muted
        };
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(21.0),
            icon_col,
        );
        // Unread LIKE pill, top-right of the glyph.
        if badge > 0 {
            let c = egui::pos2(rect.center().x + 12.0, rect.center().y - 11.0);
            p.circle_filled(c, 8.0, LIKE);
            p.text(
                c,
                Align2::CENTER_CENTER,
                badge.min(99).to_string(),
                FontId::proportional(9.5),
                Color32::WHITE,
            );
        }
        // Tooltip flyout naming the section + shortcut ("Chat  ⌘1").
        let resp = resp.on_hover_text(format!("{label}  {}{shortcut}", Self::cmd()));
        if resp.clicked() {
            self.set_tab(ui.ctx(), tab);
        }
        ui.add_space(4.0);
    }

    /// Switch the active tab. Resets the cached scroll offset so the collapsing
    /// large-title (§4b/§4d) never opens a fresh tab pre-collapsed, and re-arms the
    /// Activity poll when entering that tab.
    fn set_tab(&mut self, ctx: &egui::Context, tab: Tab) {
        if self.state.tab != tab {
            self.state.tab = tab;
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("view-scroll-y"), 0.0_f32));
        }
        if tab == Tab::Activity {
            self.next_activity = 0.0;
        }
    }

    /// A rail entry that fires an action (rather than switching tabs). Kept for an
    /// optional "launch external Godot/exported Verse" path (the in-app Verse tab is
    /// the default now).
    #[allow(dead_code)]
    fn rail_action(&mut self, ui: &mut egui::Ui, theme: &Theme, icon: &str, label: &str) -> egui::Response {
        let resp = egui::Frame::none()
            .rounding(12.0)
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::hover());
                    ui.painter()
                        .text(r.center(), Align2::CENTER_CENTER, icon, FontId::proportional(20.0), theme.muted);
                    ui.add_space(10.0);
                    ui.label(RichText::new(label).size(15.0).color(theme.muted));
                });
            })
            .response
            .interact(egui::Sense::click());
        if resp.hovered() {
            ui.painter()
                .rect_filled(resp.rect, 12.0, Color32::from_white_alpha(if theme.light { 16 } else { 12 }));
        }
        ui.add_space(4.0);
        resp
    }

    /// Open Hey Verse: launch the BUNDLED Godot engine on the desktop verse-game
    /// project — the same world as mobile, in a landscape window. The engine +
    /// project ship inside the app, so there is no separate download.
    pub fn launch_verse(&mut self, now: f64) {
        use std::path::PathBuf;
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));

        // The Godot 4.6 engine: $HEY_GODOT_BIN, else the bundled desktop/tools/godot.
        let godot = {
            let mut c: Vec<PathBuf> = Vec::new();
            if let Ok(p) = std::env::var("HEY_GODOT_BIN") {
                if !p.is_empty() {
                    c.push(PathBuf::from(p));
                }
            }
            if let Some(d) = &exe_dir {
                c.push(d.join("../../../tools/godot/godot"));
            }
            c.into_iter().find_map(|p| p.canonicalize().ok())
        };
        // The Godot project dir (must contain project.godot): $HEY_VERSE_PROJECT,
        // else the repo's mobile/hey-verse.
        let project = {
            let mut c: Vec<PathBuf> = Vec::new();
            if let Ok(p) = std::env::var("HEY_VERSE_PROJECT") {
                if !p.is_empty() {
                    c.push(PathBuf::from(p));
                }
            }
            if let Some(d) = &exe_dir {
                c.push(d.join("../../../verse-game")); // the bundled landscape copy
            }
            c.into_iter()
                .find_map(|p| p.join("project.godot").exists().then(|| p.canonicalize().ok()).flatten())
        };

        if let (Some(g), Some(p)) = (&godot, &project) {
            match std::process::Command::new(g)
                .arg("--path")
                .arg(p)
                .arg("--resolution")
                .arg("1280x720") // landscape desktop window
                .arg("--rendering-driver")
                .arg("opengl3") // the project's GL Compatibility renderer
                .spawn()
            {
                Ok(_) => {
                    self.state.toast = Some(("Opening Hey Verse…".into(), now + 2.0));
                    return;
                }
                Err(e) => log::warn!("verse (godot) launch failed: {e}"),
            }
        }

        // Fallback: a standalone exported/native verse binary beside us.
        let mut bins: Vec<PathBuf> = Vec::new();
        if let Ok(p) = std::env::var("HEY_VERSE_BIN") {
            if !p.is_empty() {
                bins.push(PathBuf::from(p));
            }
        }
        if let Some(d) = &exe_dir {
            bins.push(d.join("hey-verse"));
        }
        for path in bins {
            if path.exists() && std::process::Command::new(&path).spawn().is_ok() {
                self.state.toast = Some(("Opening Hey Verse…".into(), now + 2.0));
                return;
            }
        }
        self.state.toast = Some((
            "Hey Verse needs the Godot engine (set HEY_GODOT_BIN)".into(),
            now + 5.0,
        ));
    }

    /// Quiet per-pane header (the LIST-COLUMN-HEADER pattern) — replaces the retired
    /// collapsing large-title (the strongest iPad tell). A stable hairline-bottomed
    /// strip: a caps section eyebrow (left) + the always-present notification bell and
    /// the Feed "New post" primary action (right). No collapse-fade.
    fn content_header(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let title = match self.state.tab {
            Tab::Chat => "CHAT",
            Tab::Feed => "FEED",
            Tab::Wallet => "WALLET",
            Tab::Verse => "VERSE",
            Tab::Calls => "CALLS",
            Tab::Activity => "ACTIVITY",
            Tab::Profile => "YOU",
        };
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            // Section eyebrow: uppercase, smaller, medium — egui has no letter-spacing
            // so we lean on caps + size (the spec's eyebrow convention).
            ui.label(
                RichText::new(title)
                    .size(12.5)
                    .family(crate::icons::semibold())
                    .color(theme.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Always-present notification bell (far right) + unread badge.
                let n = self.state.notifs.len();
                let bell =
                    crate::views::icon_button(ui, theme, crate::icons::NOTIFICATIONS, 19.0, theme.muted)
                        .on_hover_text("Notifications");
                if n > 0 {
                    let c = bell.rect.right_top() + egui::vec2(-7.0, 7.0);
                    ui.painter().circle_filled(c, 7.0, LIKE);
                    ui.painter().text(
                        c,
                        Align2::CENTER_CENTER,
                        n.min(9).to_string(),
                        FontId::proportional(9.0),
                        Color32::WHITE,
                    );
                }
                if bell.clicked() {
                    self.state.show_notifs = !self.state.show_notifs;
                    if self.state.show_notifs {
                        self.load_activity();
                    }
                }

                // Feed-only "New post" → flat gold capsule pill.
                if self.state.tab == Tab::Feed {
                    ui.add_space(8.0);
                    if crate::views::pill_button(
                        ui,
                        theme,
                        &format!("{}  New post", crate::icons::ADD),
                    )
                    .clicked()
                    {
                        self.state.modal = Some(Modal::Composer);
                    }
                }
            });
        });
        ui.add_space(8.0);
        // Stable bottom hairline (always present — not the collapse-fade separator).
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.cursor().top(),
            egui::Stroke::new(1.0, theme.glass_border),
        );
        ui.add_space(12.0);
    }

    /// The 34px top CHROME strip: a clickable breadcrumb (section ▸ context) on the
    /// left, a centered ⌘K search/command stub (a button that will open the palette
    /// in a later phase), and theme + settings affordances on the right. The OS draws
    /// its own min/close above this on Win/Linux; on macOS the traffic-lights inset
    /// is handled by the transparent titlebar (we don't draw window controls).
    fn chrome_strip(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let (section, context) = self.breadcrumb();
        ui.horizontal_centered(|ui| {
            // macOS traffic-light inset so the breadcrumb clears the window buttons.
            if cfg!(target_os = "macos") {
                ui.add_space(TOP_INSET + 36.0);
            }
            // Breadcrumb: "Hey · Section" + optional "▸ Context".
            ui.label(
                RichText::new("Hey")
                    .size(22.0)
                    .family(crate::icons::semibold())
                    .color(theme.gold_ink),
            );
            ui.add_space(4.0);
            ui.label(RichText::new("·").size(13.0).color(theme.faint));
            let sect = ui
                .label(
                    RichText::new(section)
                        .size(13.0)
                        .family(crate::icons::semibold())
                        .color(theme.ink),
                )
                .interact(egui::Sense::click());
            if sect.clicked() {
                // Clicking the section name clears any open context (back to the index).
                self.state.open_chat = None;
            }
            if let Some(ctx_label) = context {
                ui.label(RichText::new(crate::icons::CHEVRON_RIGHT).size(13.0).color(theme.faint));
                ui.label(RichText::new(ctx_label).size(13.0).color(theme.muted));
            }

            // Right cluster: theme toggle + settings, then the centered ⌘K stub.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::views::icon_button(ui, theme, crate::icons::SETTINGS, 17.0, theme.muted)
                    .on_hover_text(format!("Settings  {},", Self::cmd()))
                    .clicked()
                {
                    self.state.modal = Some(crate::state::Modal::Settings);
                }
                let theme_glyph = if self.state.light {
                    crate::icons::VISIBILITY
                } else {
                    crate::icons::VISIBILITY_OFF
                };
                if crate::views::icon_button(ui, theme, theme_glyph, 17.0, theme.muted)
                    .on_hover_text(format!("Toggle theme  {}D", Self::cmd()))
                    .clicked()
                {
                    self.state.light = !self.state.light;
                    crate::theme::save_pref(self.state.light);
                }
                // Command bar removed — the palette is still on {cmd}K and the bottom hint.
            });
        });
        // Bottom hairline under the chrome strip.
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.max_rect().bottom() - 0.5,
            egui::Stroke::new(1.0, theme.glass_border),
        );
    }

    /// The 26px bottom STATUS/HINT strip: self avatar + short DID (copy-on-click) on
    /// the left, the rich connection/peers/e2e readout (moved VERBATIM from the old
    /// rail footer) in the center, and a ⌘K hint on the right. This puts network
    /// truth in permanent chrome.
    fn status_strip(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        use crate::icons;
        let online = self.state.online;
        let peers = self.state.peers;
        let dp = self.state.direct_peers;
        let plural = if peers == 1 { "" } else { "s" };
        let (dot, icon, label, sub) = if !online {
            (theme.gold_ink, icons::SWAP_HORIZ, "Connecting…", String::new())
        } else if peers == 0 {
            (theme.muted, icons::PUBLIC, "No peers", "online · 0 connected".to_string())
        } else if dp >= peers {
            (theme.good, icons::BOLT, "Direct", format!("{peers} peer{plural} · peer-to-peer"))
        } else if dp > 0 {
            (theme.good, icons::BOLT, "Mostly direct", format!("{dp}/{peers} peers direct"))
        } else {
            (theme.gold_ink, icons::HUB, "Relay-assisted", format!("{peers} peer{plural} · via relay"))
        };
        let tip = if !online {
            "Connecting to the Hey carrier…"
        } else if peers == 0 {
            "No peers connected right now — there's no live connection to measure. Your node is reachable; open a chat or follow someone to connect."
        } else if dp >= peers {
            "Direct — every live peer is connected peer-to-peer. The relay only introduced you."
        } else if dp > 0 {
            "Mixed — some peers are direct (peer-to-peer), others ride the relay. Hey keeps upgrading relayed links to direct."
        } else {
            "Relay-assisted — this network (e.g. a VPN) blocks direct links, so your (still end-to-end \
             encrypted) data rides the relay. Hey keeps trying to upgrade to a direct path."
        };
        // Top hairline above the strip.
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.max_rect().top() + 0.5,
            egui::Stroke::new(1.0, theme.glass_border),
        );
        ui.horizontal_centered(|ui| {
            // Left: self avatar (20) + short DID — copy-on-click.
            {
                let av = 20.0;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(av, av), egui::Sense::hover());
                let mut child = ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center), None);
                let avatar_cid = self
                    .state
                    .profile
                    .get("avatar")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                crate::views::avatar(
                    &mut self.media, &self.engine, &self.ev_tx, &mut child,
                    &avatar_cid, &self.state.me_did, av,
                );
            }
            ui.add_space(6.0);
            let short = if self.state.me_did.is_empty() {
                "…".to_string()
            } else {
                self.state.me_did.clone()
            };
            let did_resp = ui
                .label(
                    RichText::new(crate::state::AppState::short_did(&short))
                        .text_style(egui::TextStyle::Monospace)
                        .color(theme.muted),
                )
                .interact(egui::Sense::click())
                .on_hover_text("Copy your DID");
            if did_resp.clicked() && !self.state.me_did.is_empty() {
                let d = self.state.me_did.clone();
                ui.output_mut(|o| o.copied_text = d);
                let now = ui.ctx().input(|i| i.time);
                self.state.toast = Some(("DID copied".into(), now + 2.0));
            }

            // Center: the rich connection readout (dot + glyph + label + sub).
            ui.add_space(16.0);
            let resp = ui
                .scope(|ui| {
                    ui.horizontal_centered(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, dot);
                        ui.add_space(4.0);
                        ui.label(RichText::new(icon).size(12.0).color(dot));
                        ui.add_space(3.0);
                        ui.label(RichText::new(label).size(12.0).strong().color(dot));
                        if !sub.is_empty() {
                            ui.label(RichText::new("·").size(12.0).color(theme.faint));
                            ui.label(RichText::new(&sub).size(12.0).color(theme.muted));
                        }
                        // e2e readout — always-true badge (every link is end-to-end).
                        ui.label(RichText::new("·").size(12.0).color(theme.faint));
                        ui.label(RichText::new(icons::LOCK).size(11.0).color(theme.good));
                        ui.label(RichText::new("e2e").size(12.0).color(theme.muted));
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_text(tip);
            if resp.clicked() {
                self.state.modal = Some(crate::state::Modal::Connection);
            }

            // Right: a clickable ⌘K hint that opens the Command Palette.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let hint = ui
                    .label(
                        RichText::new(format!("{}K  commands", Self::cmd()))
                            .size(11.5)
                            .color(theme.faint),
                    )
                    .interact(egui::Sense::click())
                    .on_hover_text("Search or run a command");
                if hint.clicked() {
                    self.toggle_palette();
                }
            });
        });
    }

    /// Build the chrome-strip breadcrumb: (section name, optional context). The
    /// context is the in-section focus (e.g. the open conversation's name in Chat).
    fn breadcrumb(&self) -> (&'static str, Option<String>) {
        let section = match self.state.tab {
            Tab::Chat => "Chat",
            Tab::Feed => "Feed",
            Tab::Wallet => "Wallet",
            Tab::Verse => "Verse",
            Tab::Calls => "Calls",
            Tab::Activity => "Activity",
            Tab::Profile => "You",
        };
        let context = match self.state.tab {
            Tab::Chat => self.state.open_chat.as_ref().map(|c| c.name.clone()),
            _ => None,
        };
        (section, context)
    }

    /// Quiet placeholder for the Calls section (the full history/start surface lands
    /// in a later phase). Keeps the spine slot live + the chassis complete; the active
    /// call still uses the full-screen `views::call` overlay.
    fn calls_placeholder(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        ui.add_space(ui.available_height() * 0.28);
        crate::views::empty_state(
            ui,
            theme,
            crate::icons::CALL,
            "Calls",
            "Voice and video calls live here. Start one from a chat for now.",
        );
    }

    fn overlays(&mut self, ctx: &egui::Context, theme: &Theme) {
        // Fading backdrop dim behind any overlay/modal — gives every popup a smooth
        // entrance (and exit) over a dimmed canvas. Painted on a Background-order
        // layer created after the panels, so it sits over content but under windows.
        let active = self.state.modal.is_some()
            || self.state.viewed.is_some()
            || self.state.zoom_cid.is_some()
            || self.state.tip.open
            || self.state.palette.is_some()
            || self.state.cheat_sheet;
        let dim = ctx.animate_bool_with_time(egui::Id::new("modal-dim"), active, 0.16);
        if dim > 0.002 {
            // Scrim alpha per §5g: ~160 dark / 120 light.
            let peak = if theme.light { 120.0 } else { 160.0 };
            let p = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("modal-backdrop"),
            ));
            p.rect_filled(
                ctx.screen_rect(),
                0.0,
                Color32::from_black_alpha((peak * dim) as u8),
            );
        }
        // Publish the iPad-sheet slide-up entrance (§5g) so each sheet (owned by the
        // view/sheet agents) anchors CENTER_CENTER and offsets y by `sheet_rise()`.
        // anim 0→1 over 0.16s while a modal is open; the sheet reads it via
        // `crate::app::sheet_rise(ctx)`.
        let modal_open = self.state.modal.is_some();
        let sheet_anim = ctx.animate_bool_with_time(egui::Id::new("sheet"), modal_open, 0.16);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("sheet-anim"), sheet_anim));

        if self.state.viewed.is_some() {
            views::user_profile::ui(self, ctx, theme);
        }
        if self.state.zoom_cid.is_some() {
            views::feed::zoom_viewer(self, ctx, theme);
        }
        match self.state.modal.clone() {
            Some(Modal::Composer) => views::composer::ui(self, ctx, theme),
            Some(Modal::AddContact) => views::chat_sheets::add_contact(self, ctx, theme),
            Some(Modal::NewGroup) => views::chat_sheets::new_group(self, ctx, theme),
            Some(Modal::EditProfile) => views::profile_sheets::edit_profile(self, ctx, theme),
            Some(Modal::MyQr) => views::profile_sheets::my_qr(self, ctx, theme),
            Some(Modal::AddFriend) => views::profile_sheets::add_friend(self, ctx, theme),
            Some(Modal::Settings) => views::profile_sheets::settings(self, ctx, theme),
            Some(Modal::Connection) => views::profile_sheets::connection(self, ctx, theme),
            Some(Modal::About) => views::profile_sheets::about(self, ctx, theme),
            Some(Modal::ChatInfo(chat)) => views::chat::chat_info_sheet(self, ctx, theme, &chat),
            None => {}
        }
        // The Tip sheet floats over ANY tab (it is opened from feed / chat / profile).
        if self.state.tip.open {
            views::wallet::tip_sheet(self, ctx, theme);
        }
        // Full-screen chat-attachment image viewer (over EVERYTHING — it draws its own
        // dark backdrop). Opened by tapping an inline chat image.
        if self.state.att_viewer.is_some() {
            views::chat::attachment_viewer(self, ctx, theme);
        }
        // The Command Palette (Ctrl/Cmd+K) floats over the dim scrim, topmost peel of
        // the Esc ladder. It resolves to a PaletteAction we apply against &mut self.
        if self.state.palette.is_some() {
            if let Some(action) = views::palette::ui(&mut self.state, ctx, theme) {
                self.apply_palette_action(ctx, action);
            }
        }
        // The "?" keyboard cheat-sheet (palette-styled help card).
        if self.state.cheat_sheet && views::palette::cheat_sheet(ctx, theme) {
            self.state.cheat_sheet = false;
        }
    }

    fn toast_overlay(&mut self, ctx: &egui::Context, theme: &Theme) {
        let Some((msg, _)) = self.state.toast.clone() else { return };
        if msg.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -110.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(theme.sheet_bg)
                    .stroke(egui::Stroke::new(1.0, theme.glass_border))
                    .rounding(14.0)
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).size(13.0).color(theme.ink));
                    });
            });
    }
}

/// The small top-right notifications popup opened by the bell. Not full-screen:
/// a compact frosted card with recent notifications + new followers (follow-back).
fn notifs_popup(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if !app.state.show_notifs {
        return;
    }
    let notifs: Vec<Value> = app.state.notifs.iter().rev().cloned().collect();
    let followers = app.state.followers.clone();
    let following_dids: Vec<String> = app
        .state
        .following
        .iter()
        .filter_map(|f| f.get("did").and_then(Value::as_str))
        .map(str::to_string)
        .collect();

    let mut close = false;
    let mut open_did: Option<String> = None;
    let mut fb_did: Option<String> = None;

    egui::Window::new("notifs-popup")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 58.0))
        .frame(theme.floating(14.0))
        .show(ctx, |ui| {
            ui.set_width(322.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Notifications")
                        .size(15.0)
                        .family(crate::icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::views::icon_button(ui, theme, crate::icons::CLOSE, 16.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().top(),
                egui::Stroke::new(1.0, theme.glass_border),
            );
            ui.add_space(8.0);

            if notifs.is_empty() && followers.is_empty() {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(crate::icons::NOTIFICATIONS).size(30.0).color(theme.faint));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Nothing new")
                            .size(13.0)
                            .family(crate::icons::semibold())
                            .color(theme.muted),
                    );
                    ui.label(
                        RichText::new("Share your invite so people can follow you.")
                            .size(11.0)
                            .color(theme.faint),
                    );
                });
                ui.add_space(12.0);
                return;
            }

            egui::ScrollArea::vertical()
                .max_height(400.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for nf in &notifs {
                        let kind = nf.get("kind").and_then(Value::as_str).unwrap_or("");
                        let title = nf.get("title").and_then(Value::as_str).unwrap_or("");
                        let body = nf.get("body").and_then(Value::as_str).unwrap_or("");
                        let did = nf.get("did").and_then(Value::as_str).unwrap_or("").to_string();
                        let glyph = match kind {
                            "post" => crate::icons::PHOTO_CAMERA,
                            "follow" => crate::icons::PERSON,
                            // A mention ("mentioned you in a post") gets the chat-bubble
                            // glyph, matching the Android "mentioned you" treatment.
                            "mention" => crate::icons::CHAT_BUBBLE_OUTLINE,
                            _ => crate::icons::NOTIFICATIONS,
                        };
                        let clicked = crate::views::list_row(ui, theme, false, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(glyph).size(17.0).color(theme.gold_ink));
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    if !title.is_empty() {
                                        ui.label(
                                            RichText::new(title)
                                                .size(13.0)
                                                .family(crate::icons::semibold())
                                                .color(theme.ink),
                                        );
                                    }
                                    if !body.is_empty() {
                                        ui.label(RichText::new(body).size(11.0).color(theme.muted));
                                    }
                                });
                            });
                        })
                        .clicked();
                        if clicked && !did.is_empty() {
                            open_did = Some(did);
                        }
                    }

                    if !followers.is_empty() {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Followers")
                                .size(11.0)
                                .family(crate::icons::semibold())
                                .color(theme.muted),
                        );
                        ui.add_space(4.0);
                        for f in &followers {
                            let did = f.get("did").and_then(Value::as_str).unwrap_or("").to_string();
                            if did.is_empty() {
                                continue;
                            }
                            let already = following_dids.contains(&did);
                            let mut this_fb = false;
                            let clicked = crate::views::list_row(ui, theme, false, |ui| {
                                ui.horizontal(|ui| {
                                    crate::views::avatar(&mut app.media, &app.engine, &app.ev_tx, ui, "", &did, 30.0);
                                    ui.add_space(8.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(AppState::short_did(&did))
                                                .size(13.0)
                                                .family(crate::icons::semibold())
                                                .color(theme.ink),
                                        );
                                        ui.label(RichText::new("started following you").size(11.0).color(theme.muted));
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if already {
                                            ui.label(RichText::new("Following").size(11.0).color(theme.muted));
                                        } else if crate::views::pill_button(ui, theme, "Follow back").clicked() {
                                            this_fb = true;
                                        }
                                    });
                                });
                            })
                            .clicked();
                            if this_fb {
                                fb_did = Some(did);
                            } else if clicked {
                                open_did = Some(did);
                            }
                        }
                    }
                });
        });

    if let Some(did) = fb_did {
        app.follow_back(&did);
        app.load_activity();
    } else if let Some(did) = open_did {
        app.state.viewed = Some(crate::state::ViewedUser { did: did.clone(), ..Default::default() });
        app.load_user(&did);
        app.state.show_notifs = false;
    }
    if close {
        app.state.show_notifs = false;
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn persist_egui_memory(&self) -> bool {
        // Don't carry egui state across launches (so the welcome always starts on
        // page 0 + no stale UI state leaks between runs).
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events(ctx);
        self.poll(ctx);

        // Publish the local DID EARLY each frame so `views::avatar` can draw the
        // gold self-ring when it renders our own avatar anywhere in the tree.
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("me-did"), self.state.me_did.clone()));

        // ── GLOBAL KEYMAP (read BEFORE the panels, mirroring the Esc block) ─────────
        // Single-key actions are GATED on `no_focus` so typing in any TextEdit (the
        // composer, the palette field, any search) is never hijacked. Ctrl/Cmd chords
        // are safe to read while a field is focused (they don't clash with text).
        //
        // IMPORTANT: while the palette is OPEN it reads its OWN keys (arrows / Enter /
        // Cmd+N = move) inside `views::palette::ui` later this frame — so we read ONLY
        // Cmd+K here and leave every other key un-consumed for the palette. Otherwise
        // this block runs the full global map.
        let palette_open = self.state.palette.is_some();
        // Cmd+K toggles the palette (open OR close) — always live, read first.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.toggle_palette();
        }
        if !palette_open {
            let no_focus = ctx.memory(|m| m.focused().is_none());
            let (cmd_d, cmd_comma, cmd_bslash, cmd_b, cmd_n, sec) = ctx.input_mut(|i| {
                let md = egui::Modifiers::COMMAND;
                // Ctrl/Cmd+1..6 → sections. Consume each so a focused field can't eat them.
                let mut section: Option<Tab> = None;
                for (key, tab) in [
                    (egui::Key::Num1, Tab::Chat),
                    (egui::Key::Num2, Tab::Feed),
                    (egui::Key::Num3, Tab::Wallet),
                    (egui::Key::Num4, Tab::Verse),
                    (egui::Key::Num5, Tab::Calls),
                    (egui::Key::Num6, Tab::Profile),
                ] {
                    if i.consume_key(md, key) {
                        section = Some(tab);
                    }
                }
                (
                    i.consume_key(md, egui::Key::D),
                    i.consume_key(md, egui::Key::Comma),
                    i.consume_key(md, egui::Key::Backslash),
                    i.consume_key(md, egui::Key::B),
                    i.consume_key(md, egui::Key::N),
                    section,
                )
            });
            if let Some(tab) = sec {
                self.set_tab(ctx, tab);
            }
            if cmd_d {
                self.state.light = !self.state.light;
                crate::theme::save_pref(self.state.light);
            }
            if cmd_comma {
                self.state.modal = Some(crate::state::Modal::Settings);
            }
            if cmd_bslash {
                // Toggle Info panel — placeholder state until P4 builds the panel.
                self.state.show_info = !self.state.show_info;
            }
            if cmd_b {
                // Toggle the list column — placeholder until the P4 splitter lands.
                self.state.list_collapsed = !self.state.list_collapsed;
            }
            if cmd_n {
                // Context-new: New post (the only wired "new" surface so far).
                self.state.modal = Some(Modal::Composer);
            }
            // "?" cheat-sheet — single key, gated on no field focus (Shift+/ commonly).
            if no_focus && ctx.input(|i| i.key_pressed(egui::Key::Questionmark)) {
                self.state.cheat_sheet = true;
            }
        }

        // Esc closes the topmost overlay (desktop affordance). Palette is the topmost
        // peel, then the cheat-sheet, then the existing ladder.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.state.palette.is_some() {
                self.state.palette = None;
            } else if self.state.cheat_sheet {
                self.state.cheat_sheet = false;
            } else if self.state.att_viewer.is_some() {
                self.state.att_viewer = None;
            } else if self.state.zoom_cid.is_some() {
                self.state.zoom_cid = None;
            } else if self.state.tip.open && self.state.tip.stage != crate::state::TipStage::Sending {
                self.state.tip = crate::state::TipForm::default();
            } else if self.state.react_target.is_some() {
                self.state.react_target = None;
            } else if self.state.to_delete.is_some() {
                self.state.to_delete = None;
                self.state.block_when_deleting = false;
            } else if self.state.viewed.is_some() {
                self.state.viewed = None;
            } else if self.state.modal.is_some() {
                self.state.modal = None;
            } else if self.state.show_info {
                self.state.show_info = false;
            }
        }

        let theme = Theme::get(self.state.light);
        theme.apply(ctx);

        // Full-window gradient + glow on the background layer (behind all panels).
        theme.paint_background(&ctx.layer_painter(egui::LayerId::background()), ctx.screen_rect());

        if !self.state.onboarded {
            // First-run welcome: create-new vs restore-from-phrase (Android WelcomeFlow).
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(24.0, 16.0)))
                .show(ctx, |ui| views::welcome::ui(self, ui, &theme));
            self.toast_overlay(ctx, &theme);
        } else if !self.state.booted {
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() * 0.4);
                        ui.label(
                            RichText::new("Hey")
                                .color(theme.gold_ink)
                                .size(56.0)
                                .family(crate::icons::display()),
                        );
                        ui.add_space(12.0);
                        ui.spinner();
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("Starting your on-device runtime…")
                                .color(theme.muted)
                                .size(13.0),
                        );
                    });
                });
        } else {
            // ── 34px top CHROME strip: breadcrumb (section ▸ context) · ⌘K stub ·
            //    theme/settings affordance. Added FIRST so it spans the full window
            //    width above the spine (the OS draws min/close above this).
            egui::TopBottomPanel::top("chrome-strip")
                .exact_height(44.0)
                .frame(
                    egui::Frame::none()
                        .fill(theme.bg2)
                        .inner_margin(egui::Margin::symmetric(12.0, 0.0)),
                )
                .show(ctx, |ui| self.chrome_strip(ui, &theme));

            // ── 26px bottom STATUS/HINT strip: self DID · connection/peers/e2e
            //    readout (moved from the old rail footer) · ⌘K hint. Spans full width.
            egui::TopBottomPanel::bottom("status-strip")
                .exact_height(26.0)
                .frame(
                    egui::Frame::none()
                        .fill(theme.bg2)
                        .inner_margin(egui::Margin::symmetric(12.0, 0.0)),
                )
                .show(ctx, |ui| self.status_strip(ui, &theme));

            // 56px icon SPINE anchored to the left edge — the flagship chassis edge
            // (identity dot top · sections · spacer · You-avatar + Settings bottom).
            egui::SidePanel::left("nav-rail")
                .exact_width(56.0)
                .resizable(false)
                .show_separator_line(true)
                .frame(theme.spine_frame())
                .show(ctx, |ui| {
                    ui.set_min_height(ui.available_height());
                    // Etched-aluminum top-edge highlight (dark-only) on the recessed spine.
                    theme.etch_top(ui.painter(), ui.max_rect());
                    self.rail(ui, &theme);
                });

            // Content area — every tab owns the FULL width and lays out its own
            // desktop master-detail / multi-pane layout (like Chat). No centred
            // phone-width column.
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(28.0, 14.0)))
                .show(ctx, |ui| {
                    self.content_header(ui, &theme);
                    match self.state.tab {
                        Tab::Chat => views::chat::ui(self, ui, &theme),
                        Tab::Feed => views::feed::ui(self, ui, &theme),
                        Tab::Wallet => views::wallet::ui(self, ui, &theme),
                        Tab::Verse => views::verse::ui(self, ui, &theme),
                        Tab::Calls => self.calls_placeholder(ui, &theme),
                        Tab::Activity => views::activity::ui(self, ui, &theme),
                        Tab::Profile => views::profile::ui(self, ui, &theme),
                    }
                });

            // Overlays + sheets, layered on top (each owns its own Window/Area).
            self.overlays(ctx, &theme);
            notifs_popup(self, ctx, &theme);
            self.toast_overlay(ctx, &theme);
            // The in-call overlay sits ABOVE everything (its own Foreground scrim).
            views::call::ui(self, ctx, &theme);
        }

        // Dev self-capture: once the UI has settled, grab the framebuffer + exit.
        if let Some(path) = self.shot.clone() {
            ctx.request_repaint();
            let t = ctx.input(|i| i.time);
            if !self.shot_requested && self.state.booted && t > 3.0 {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            if self.shot_requested {
                let img = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(image) = img {
                    let [w, h] = image.size;
                    let mut rgba = Vec::with_capacity(w * h * 4);
                    for p in &image.pixels {
                        rgba.extend_from_slice(&p.to_array());
                    }
                    if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                        let _ = buf.save(&path);
                        log::info!("screenshot saved to {path} ({w}x{h})");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }

        // Keep the loop alive for polling + in-flight results (egui only repaints
        // on input otherwise).
        let wait = if self.engine.inflight() > 0 { 120 } else { 500 };
        ctx.request_repaint_after(Duration::from_millis(wait));
    }
}

// ── Calls (voice/video, 1:1, direct-only) — signaling + media lifecycle ───────
//
// Signaling rides the E2E DM channel (social::call_send / call_poll). Media is
// direct P2P over the carrier ALPN (hey_mobile_runtime::voice_*/video_*). Media
// must be started/stopped EXPLICITLY on Active enter/leave (no DisposableEffect).
// The cpal audio pump + nokhwa/openh264 video pump attach in start_media/stop_media.

/// Caller-minted, echoed-unchanged call id. Avoids a uuid dep — nanos are unique enough.
fn mint_call_id() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("call-{n:x}")
}

impl App {
    /// Build + send a call-control signal (offer/accept/decline/end) over the DM lane.
    fn send_call_sig(&self, to: &str, ty: &str, call_id: &str, video: bool) {
        let mut o = serde_json::json!({ "type": ty, "call_id": call_id });
        if video {
            o["video"] = Value::Bool(true);
        }
        let (to, payload) = (to.to_string(), o.to_string());
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::call_send(&to, &payload).await },
            |_ok| UiEvent::Toast(String::new()),
        );
    }

    /// Begin an OUTGOING call to a contact. No-op unless Idle + online.
    pub fn start_call(&mut self, did: String, name: String, video: bool) {
        if !matches!(self.state.call, CallState::Idle) || did.is_empty() || !self.state.online {
            return;
        }
        let call_id = mint_call_id();
        self.send_call_sig(&did, "offer", &call_id, video);
        self.state.call = CallState::Outgoing { peer: did, name, call_id, video };
    }

    /// Accept the current INCOMING call → Active + start media + tell the caller.
    pub fn accept_call(&mut self) {
        let (peer, name, call_id, video) = match self.state.call.clone() {
            CallState::Incoming { peer, name, call_id, video } => (peer, name, call_id, video),
            _ => return,
        };
        self.send_call_sig(&peer, "accept", &call_id, video);
        self.state.call =
            CallState::Active { peer, name, call_id, video, is_caller: false };
        self.call_since = Some(std::time::Instant::now());
        self.start_media();
    }

    /// Decline the current INCOMING call.
    pub fn decline_call(&mut self) {
        if let CallState::Incoming { peer, call_id, .. } = self.state.call.clone() {
            self.send_call_sig(&peer, "decline", &call_id, false);
            self.end_local(&call_id);
        }
    }

    /// Cancel/hang up from ANY non-idle state.
    pub fn hangup(&mut self) {
        let (peer, call_id) = match self.state.call.clone() {
            CallState::Outgoing { peer, call_id, .. }
            | CallState::Incoming { peer, call_id, .. }
            | CallState::Active { peer, call_id, .. } => (peer, call_id),
            CallState::Idle => return,
        };
        self.send_call_sig(&peer, "end", &call_id, false);
        self.end_local(&call_id);
    }

    /// Local teardown shared by decline/hangup/remote-end. Idempotent.
    fn end_local(&mut self, call_id: &str) {
        self.state.last_ended_call_id = Some(call_id.to_string());
        self.stop_media();
        self.state.call = CallState::Idle;
        self.state.call_muted = false;
        self.state.call_cam_off = false;
    }

    /// Start the carrier media session for the current Active call. Ticket resolution
    /// runs on an engine worker (needs hey-core thread-locals); the runtime wrapper
    /// then spawns the dial/recv loops on the carrier runtime. (cpal/openh264 pumps
    /// attach here in the media phases.)
    fn start_media(&mut self) {
        let (peer, is_caller, video) = match &self.state.call {
            CallState::Active { peer, is_caller, video, .. } => (peer.clone(), *is_caller, *video),
            _ => return,
        };
        self.state.call_muted = false;
        self.state.call_cam_off = false;
        // Open the local cpal audio pump (+ camera/decode threads for video) NOW, on
        // the UI thread — cpal's `Stream` is `!Send` so it cannot be built on an
        // engine worker. The pump only *sends/receives* once the carrier link is up
        // (voice_send/recv are no-ops with no connected peer), so opening it before
        // the dial completes is safe and avoids a first-second of dropped audio.
        self.call_media = Some(crate::call_media::CallMedia::start(video));
        // Resolve the peer ticket + open the carrier media session on an engine
        // worker (needs hey-core thread-locals); the runtime wrapper then spawns the
        // dial/recv loops on the carrier runtime.
        self.engine.call(
            &self.ev_tx,
            move || async move {
                let ticket = hey_mobile_runtime::social::peer_ticket(&peer).await;
                let empty = ticket.is_empty();
                if !empty {
                    hey_mobile_runtime::voice_start(ticket.clone(), is_caller);
                    if video {
                        hey_mobile_runtime::video_start(ticket);
                    }
                }
                empty
            },
            |empty| {
                if empty {
                    UiEvent::Error("No carrier ticket for this contact".into())
                } else {
                    UiEvent::Toast(String::new())
                }
            },
        );
    }

    /// Stop the carrier media session AND tear down the local cpal/camera pump.
    /// Dropping `call_media` stops the cpal streams and signals the video threads
    /// to exit; the runtime `voice_stop`/`video_stop` close the carrier links.
    fn stop_media(&mut self) {
        self.call_media = None;
        hey_mobile_runtime::voice_stop();
        hey_mobile_runtime::video_stop();
        self.call_since = None;
    }

    /// Toggle the mic mute on the live call (drives the cpal capture gate + the
    /// runtime send-side mute). No-op when no call media is up.
    pub fn toggle_mute(&mut self) {
        self.state.call_muted = !self.state.call_muted;
        if let Some(m) = &self.call_media {
            m.set_muted(self.state.call_muted);
        }
    }

    /// Toggle the camera on a video call (drives the capture-send gate + the
    /// runtime's video pause). No-op when no call media is up.
    pub fn toggle_cam(&mut self) {
        self.state.call_cam_off = !self.state.call_cam_off;
        if let Some(m) = &self.call_media {
            m.set_cam_off(self.state.call_cam_off);
        }
    }

    /// Borrow the live call media (for the overlay's local/remote preview frames).
    pub fn call_media(&self) -> Option<&crate::call_media::CallMedia> {
        self.call_media.as_ref()
    }

    /// Seconds since the current call went Active (for the in-call timer). None
    /// before connect (Outgoing/Incoming) so the overlay can show "Calling…" etc.
    pub fn call_elapsed(&self) -> Option<u64> {
        self.call_since.map(|t| t.elapsed().as_secs())
    }

    /// Poll inbound call signals (engine worker) → CallSignals event.
    fn poll_call_signals(&self) {
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::call_poll().await },
            |v| UiEvent::CallSignals(as_array(v)),
        );
    }

    /// Probe a contact's live transport (direct/relay/offline) → ContactTransport event.
    fn probe_transport(&self, did: String) {
        let d = did.clone();
        self.engine.call(
            &self.ev_tx,
            move || async move { hey_mobile_runtime::social::contact_transport(&d).await },
            move |t| UiEvent::ContactTransport { did, transport: t },
        );
    }

    /// Alert on an incoming call — raise the window (ring tone attaches in the audio phase).
    fn start_ring(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}
