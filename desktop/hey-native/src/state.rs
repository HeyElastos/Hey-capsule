//! Pure data: AppState, view-model structs, Tab/Modal enums, and the UiEvent
//! result type that flows back from the engine + receiver threads. The only egui
//! type referenced is ColorImage (a decoded-but-not-yet-uploaded media payload).

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

/// The nav tabs. Chat is the launch tab (matches Android).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Chat,
    Feed,
    Wallet,
    Verse,
    Activity,
    Profile,
}
impl Default for Tab {
    fn default() -> Self {
        Tab::Chat
    }
}

/// Sheets / dialogs layered over a tab (1:1 with the Android ModalBottomSheets).
#[derive(Clone, PartialEq)]
pub enum Modal {
    Composer,
    EditProfile,
    AddContact,
    AddFriend,
    NewGroup,
    MyQr,
    Settings,
    Connection,
    About,
    /// Chat-info sheet for the open conversation (header avatar/name tap). Carries
    /// the chat so the sheet has the recipient did/name without re-reading state.
    ChatInfo(OpenChat),
}

#[derive(Default, Clone)]
pub struct ProfileDraft {
    pub nickname: String,
    pub bio: String,
    pub avatar_cid: String,
    pub avatar_bytes: Option<Vec<u8>>, // freshly-picked, not yet uploaded
    pub busy: bool,
    pub loaded: bool,
}

#[derive(Default, Clone)]
pub struct Composer {
    pub caption: String,
    pub tiles: Vec<Value>, // media tiles {cid,mime,type,name}
    pub busy: bool,
    pub status: String,
}

/// A conversation opened over a tab (full-screen in Android; a panel here).
#[derive(Clone, PartialEq)]
pub struct OpenChat {
    pub id: String, // did (DM) or gid (group)
    pub name: String,
    pub is_group: bool,
}

/// Max attachments the composer will stage in one batch — matches Android's
/// `(staged + add).take(10)` cap. Picks beyond this are dropped (with a toast).
pub const STAGED_CAP: usize = 10;

/// A file/photo the user has picked but not yet sent — held in the composer tray
/// so it can be reviewed/removed before send (the desktop parity for Android's
/// `StagedItem`). The bytes are already host-side scaled/encoded by `process_media`
/// at pick time, so send is a straight `chat_send_attachment` with no extra work.
#[derive(Clone)]
pub struct StagedAttachment {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub name: String,
}

/// Transient inputs for the various add/follow/invite/group sheets.
#[derive(Default, Clone)]
pub struct Sheets {
    pub add_input: String,    // AddContact / AddFriend link or invite
    pub add_status: String,
    pub add_busy: bool,
    pub group_name: String,
    pub group_selected: HashSet<String>,
    pub group_status: String,
    pub group_busy: bool,
}

/// First-run welcome / onboarding (create-new vs restore-from-phrase), a port of
/// the Android WelcomeFlow.
#[derive(Default, Clone)]
pub struct OnboardingState {
    pub page: usize,         // intro pager page (0..3)
    pub restore_mode: bool,  // showing the restore-from-phrase screen
    pub profile_setup: bool, // CREATE-new only: the one-time "Set up your profile"
                             // step (== Android OnboardingScreen) shown after the
                             // identity is created and before entering the app. The
                             // nickname/bio/avatar draft lives in `profile_draft`.
    pub phrase: String,      // typed recovery phrase
    pub error: String,
    pub busy: bool,
}

/// The sovereign multi-chain wallet (a desktop port of the Android WalletScreen).
/// Keys are derived in-process by the embedded runtime; we only hold the public
/// addresses + fetched balances here. BEAM is deferred (needs the C++ lib).
#[derive(Default, Clone)]
pub struct WalletState {
    pub loaded: bool,        // addresses + chain list resolved
    pub evm_addr: String,    // 0x… (ESC + Ethereum share the secp256k1 address)
    pub ela_addr: String,    // E… (ELA mainchain, P-256)
    pub did: String,         // did:elastos:…
    pub chains: Vec<Value>,  // [{key,name,chainId,symbol}] for the EVM chains
    pub chain: String,       // selected chain key: "esc" | "ethereum" | "ela"
    pub balances: HashMap<String, Value>, // chain key -> evm_balances{} or ela_balance{}
    pub refreshing: HashSet<String>,      // chain keys with a balance fetch in-flight
    pub history: Vec<Value>,              // local tx records (newest first)
    pub show_history: bool,
    pub receive: Option<String>,          // Some(chain key) => Receive sheet open
    pub send: SendForm,                   // the Send sheet
    pub show_backup: bool,                // the recovery-phrase backup sheet
    pub phrase: Option<String>,           // BIP39 phrase, fetched on reveal, cleared on close
    pub locked: bool,                     // identity has no BIP39 seed → offer to create one
    pub creating_seed: bool,              // a create-seed op is in flight
    // Per-user hidden-token set (scam/dust protection), keyed "chainKey:contract"
    // (== Android SharedPreferences `hidden_tokens`). Local persistence — the engine
    // has no hidden-token fn. Persisted to hidden-tokens.json, loaded once on boot.
    pub hidden_tokens: HashSet<String>,
    pub show_hidden: bool,                // reveal hidden tokens in the token list
    pub show_settings: bool,              // the wallet settings sheet (gear)
}

/// What stage the Send sheet is in.
#[derive(Default, Clone, PartialEq)]
pub enum SendStage {
    #[default]
    Edit,    // entering recipient + amount
    Review,  // confirm screen (what you're about to sign)
    Sending, // broadcasting / awaiting hash
    Done,    // tx hash returned
}

/// The Send sheet's transient form.
#[derive(Default, Clone)]
pub struct SendForm {
    pub open: bool,
    pub chain: String,         // chain key the send is on
    pub token: Option<Value>,  // Some(token json) => ERC-20; None => native coin
    pub to: String,
    pub amount: String,
    pub stage: SendStage,
    pub status: String,
    pub tx_hash: String,
    // On-chain confirmation state for the Done screen, updated by the EVM tx-status
    // poll: "pending" | "success" | "failed". ELA mainchain has no receipt lookup,
    // so its Done is left "pending" with broadcast-only copy (no poll runs).
    pub conf: String,
    pub polling: bool,         // a tx-status poll worker is in flight for tx_hash
}

/// What stage the Tip sheet is in (mirrors Android's TipSheet flow).
#[derive(Default, Clone, PartialEq)]
pub enum TipStage {
    #[default]
    Resolving, // exchanging + looking up the recipient's published addresses
    Edit,      // pick a chain/asset + amount
    Review,    // confirm screen (what you're about to sign)
    Sending,   // broadcasting / awaiting hash
    Done,      // tx hash returned
}

/// The Tip sheet — a desktop port of the Android `TipSheet`. Opened from a feed
/// post (author), a DM header (contact), or a profile. Resolves the recipient's
/// published receive addresses by DID (never a hardcoded address), then sends a
/// transfer through the SAME wallet send path tagged `kind:"tip"` and notifies the
/// recipient over the DM channel.
#[derive(Default, Clone)]
pub struct TipForm {
    pub open: bool,
    pub did: String,          // recipient DID (what we resolve the address from)
    pub name: String,         // recipient display name (for the sheet header)
    pub addresses: HashMap<String, String>, // resolved {chainKey -> address}
    pub chains: Vec<(String, String)>,       // tippable (chainKey, symbol) in display order
    pub chain: String,        // selected chain key ("ela" | "esc")
    pub token: Option<Value>, // Some(token json) => ERC-20 (ESC only); None => native
    pub tokens: Vec<Value>,   // ERC-20s the sender holds on the selected EVM chain
    pub amount: String,
    pub stage: TipStage,
    pub status: String,
    pub tx_hash: String,
    pub symbol: String,       // symbol of the tx record returned on success
}

/// State for the full-screen "view another user" overlay.
#[derive(Default, Clone)]
pub struct ViewedUser {
    pub did: String,
    pub profile: Value,
    pub posts: Vec<Value>,
    pub following_them: bool,
    pub status: String,
    pub loaded: bool,
}

/// Max number of decrypted attachment byte-blobs kept resident in RAM. Chat
/// attachments are decoded media; an unbounded map grows without limit over a
/// media-heavy session, so we LRU-evict to this cap.
pub const ATT_BYTES_CAP: usize = 24;

/// Max number of attachment GPU textures kept uploaded. Each distinct attachment
/// image scrolled into view used to leak a `TextureHandle` for the whole session;
/// dropping the handle frees the VRAM, so we LRU-evict to this cap.
pub const ATT_TEX_CAP: usize = 48;

/// A small capped LRU of decoded-attachment textures, keyed by the attachment's
/// raw JSON (the same key `AppState.attachments` / `att_loading` use). Replaces the
/// unbounded `ctx.data` `insert_temp` memoisation in chat.rs. Dropping an evicted
/// `TextureHandle` releases its GPU texture.
#[derive(Default)]
pub struct AttTexCache {
    map: HashMap<String, egui::TextureHandle>,
    order: VecDeque<String>, // oldest at front
}

impl AttTexCache {
    /// Look up a cached texture (cloning the cheap handle).
    pub fn get(&self, key: &str) -> Option<egui::TextureHandle> {
        self.map.get(key).cloned()
    }

    /// Insert a freshly-uploaded texture, evicting the oldest while over `ATT_TEX_CAP`.
    pub fn insert(&mut self, key: String, tex: egui::TextureHandle) {
        if !self.map.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.map.insert(key, tex);
        while self.order.len() > ATT_TEX_CAP {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }

    /// Forget a key (e.g. on retry) so it re-uploads next time.
    pub fn remove(&mut self, key: &str) {
        if self.map.remove(key).is_some() {
            self.order.retain(|k| k != key);
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub me_did: String,
    pub friend_link: String,
    pub online: bool,
    pub direct: bool,
    pub direct_peers: i64,
    pub relay_peers: i64,
    pub peers: i64,
    // live network diagnostics (mirror Android's connection sheet)
    pub public_v4: String,
    pub public_v6: String,
    pub ipv4: bool,
    pub ipv6_global: bool,
    pub udp_v4: bool,
    pub udp_v6: bool,
    pub local_addrs: Vec<String>,
    pub tab: Tab,
    pub modal: Option<Modal>,
    pub light: bool,
    pub booted: bool, // first whoami landed

    // ── feed ────────────────────────────────────────────────────────────────
    pub feed: Vec<Value>,
    pub feed_loaded: bool,
    pub carousel: HashMap<String, usize>, // post_id -> selected tile idx
    pub reactions: HashMap<String, Value>, // post_id -> reactions summary
    pub comments: HashMap<String, Value>, // post_id -> comments array
    pub comment_draft: HashMap<String, String>, // post_id -> draft text
    pub open_comments: HashSet<String>,   // post_ids with the comment box open
    pub reply_to: HashMap<String, (String, String)>, // post_id -> (comment_id, author label)
    pub editing: Option<(String, String)>, // (post_id, draft caption)
    pub loaded_meta: HashSet<String>,     // post_ids whose reactions/comments are loaded
    pub zoom_cid: Option<String>,         // full-screen image viewer (by cid)
    pub feed_scroll_to: Option<String>,   // one-shot: scroll the feed to this post id (from profile grid)

    // ── chat ────────────────────────────────────────────────────────────────
    pub contacts: Vec<Value>,
    pub groups: Vec<Value>,
    pub chats_loaded: bool,
    pub open_chat: Option<OpenChat>,
    pub convo: Vec<Value>,
    pub chat_draft: String,
    pub unread: u32,
    // OS-notification baselines (== Android RuntimeService.lastUnread + the no-burst
    // seed). `last_unread` is the previous tick's aggregate unread; a positive delta
    // raises a "N new messages" OS notification. `notif_seeded` is false until the
    // FIRST unread poll lands, so startup (which can report a backlog of unread) seeds
    // the baseline silently instead of bursting one notification per pre-existing chat.
    pub last_unread: u32,
    pub notif_seeded: bool,
    pub chat_search: Option<String>,           // Some => search field open
    pub react_target: Option<String>,          // message id pending emoji pick
    pub edit_target: Option<String>,           // own message id being edited
    pub edit_draft: String,                    // edit-dialog text buffer
    // ── calls (voice/video, 1:1, direct-only) ─────────────────────────────────
    pub call: CallState,
    pub last_ended_call_id: Option<String>,      // ghost-ring suppression (2-min poll window)
    pub call_muted: bool,
    pub call_cam_off: bool,
    pub call_transport: HashMap<String, String>, // did -> "direct"|"relay"|"offline" (gate cache)
    pub msg_reactions: HashMap<String, Value>, // chat_id -> [MessageReaction]
    pub attachments: HashMap<String, Vec<u8>>, // attachment key -> decrypted bytes (LRU-capped; insert via put_attachment)
    pub att_order: VecDeque<String>,           // LRU order for `attachments` (oldest at front)
    pub att_tex: AttTexCache,                  // capped decoded-attachment GPU textures (chat.rs)
    pub att_loading: HashSet<String>,
    // Composer attachment tray: files/photos picked but not yet sent. Mirrors
    // Android's `staged` — review, ✕-remove, caption, then send-all (capped at
    // STAGED_CAP). `staging_busy` covers the in-flight file picker.
    pub staged: Vec<StagedAttachment>,
    pub staging_busy: bool,
    pub sending: bool,                         // a staged send-all is in flight (disables Send)
    pub send_done: usize,                      // items sent so far in the active batch
    pub send_total: usize,                     // items in the active batch ("Sending d/t…")
    pub to_delete: Option<OpenChat>,           // long-press delete confirm
    pub block_when_deleting: bool,             // the pending delete-confirm is a "Block & remove"
    pub att_viewer: Option<String>,            // attachment key whose full-screen viewer is open
    // Per-chat local prefs (== Android SharedPreferences muted_chats / blocked_dids),
    // persisted to chat-prefs.json next to the data dir and loaded once on boot.
    pub muted_chats: HashSet<String>,          // chat ids the user muted
    pub blocked_dids: HashSet<String>,         // DM dids the user blocked + removed

    // ── activity ──────────────────────────────────────────────────────────────
    pub notifs: VecDeque<Value>,
    pub followers: Vec<Value>,
    pub following: Vec<Value>,
    pub activity_loaded: bool,
    pub activity_selected: Option<String>, // (legacy) selected did
    pub show_notifs: bool,                 // the top-right bell notifications popup

    // ── profile / sheets / overlays ───────────────────────────────────────────
    pub profile: Value, // get_profile("")
    pub profile_draft: ProfileDraft,
    pub composer: Composer,
    pub sheets: Sheets,
    pub viewed: Option<ViewedUser>, // full-screen peer profile overlay

    // ── onboarding ──────────────────────────────────────────────────────────────
    pub onboarded: bool,            // false → show the first-run welcome flow
    pub onboarding: OnboardingState,

    // ── wallet ────────────────────────────────────────────────────────────────
    pub wallet: WalletState,

    // ── tipping (the TipSheet, opened from feed / chat / profile) ───────────────
    pub tip: TipForm,

    pub toast: Option<(String, f64)>, // (msg, expires_at_secs)
}

impl AppState {
    /// did:key... -> the single display character the gradient avatar shows.
    pub fn did_initial(did: &str) -> char {
        did.strip_prefix("did:key:z")
            .unwrap_or(did)
            .chars()
            .next()
            .unwrap_or('?')
            .to_ascii_uppercase()
    }

    /// Short, human-glanceable form of a DID for list rows. Slices on CHAR
    /// boundaries (not byte offsets) so a non-ASCII profile string can't panic.
    pub fn short_did(did: &str) -> String {
        let body = did.strip_prefix("did:key:z").unwrap_or(did);
        if body.chars().count() <= 12 {
            body.to_string()
        } else {
            let prefix: String = body.chars().take(6).collect();
            let suffix: String = {
                let mut last4: Vec<char> = body.chars().rev().take(4).collect();
                last4.reverse();
                last4.into_iter().collect()
            };
            format!("{prefix}…{suffix}")
        }
    }

    /// Insert a decrypted attachment blob, LRU-evicting the oldest while over
    /// `ATT_BYTES_CAP` so the decrypted-bytes map can't grow unbounded over a
    /// media-heavy session. Call this from the `AttachmentBytes` handler INSTEAD of
    /// `self.state.attachments.insert(..)`. Re-inserting an existing key refreshes its
    /// recency (moves it to the back of the eviction order).
    pub fn put_attachment(&mut self, key: String, bytes: Vec<u8>) {
        if self.attachments.contains_key(&key) {
            self.att_order.retain(|k| k != &key);
        }
        self.att_order.push_back(key.clone());
        self.attachments.insert(key, bytes);
        while self.att_order.len() > ATT_BYTES_CAP {
            if let Some(old) = self.att_order.pop_front() {
                self.attachments.remove(&old);
                self.att_tex.remove(&old); // free the matching GPU texture too
            }
        }
    }

    /// Prune the insert-only feed side-maps (reactions / comments / loaded_meta /
    /// carousel) and the per-chat `msg_reactions` map down to a set of still-relevant
    /// post ids. Call after `load_feed` with the visible post ids so a long session
    /// can't accumulate stale entries forever. Clearing `loaded_meta` for dropped ids
    /// also lets their counts re-fetch if the post scrolls back into view.
    ///
    /// `msg_reactions` is keyed by chat id, not post id, so it is intentionally left
    /// alone here; the feed agent need only pass post ids.
    pub fn retain_posts(&mut self, ids: &HashSet<String>) {
        self.reactions.retain(|k, _| ids.contains(k));
        self.comments.retain(|k, _| ids.contains(k));
        self.loaded_meta.retain(|k| ids.contains(k));
        self.carousel.retain(|k, _| ids.contains(k));
    }
}

/// Results funnelled back from the engine workers + the receiver threads onto the
/// UI thread over a std mpsc channel. Every variant is Send.
pub enum UiEvent {
    Whoami { did: String },
    FriendLink(String),
    Health { online: bool, direct: bool, direct_peers: i64, relay_peers: i64, peers: i64, public_v4: String, public_v6: String, ipv4: bool, ipv6_global: bool, udp_v4: bool, udp_v6: bool, local_addrs: Vec<String> },
    Feed(Vec<Value>),
    Reactions { post_id: String, summary: Value },
    Comments { post_id: String, list: Value },
    Contacts(Vec<Value>),
    Groups(Vec<Value>),
    Convo { id: String, msgs: Vec<Value> },
    MsgReactions { chat_id: String, list: Value },
    AttachmentBytes { key: String, bytes: Vec<u8> },
    Followers(Vec<Value>),
    Following(Vec<Value>),
    Profile(Value),
    MediaUploadedMany(Vec<Value>),   // several tiles (multi-pick) -> append
    PickedAvatarCid(String),         // avatar uploaded -> set on the profile draft
    // ── chat staged attachments ──────────────────────────────────────────────
    StagedPicked(Vec<StagedAttachment>), // picker returned -> append to the tray (capped)
    StagedProgress(usize),               // one item of the batch finished -> bump send_done
    StagedSent(OpenChat),                // the whole batch finished -> clear tray + reload convo
    ViewedProfile(Value),
    ViewedPosts(Vec<Value>),
    ViewedFollowing(bool),
    Notif(Value),
    Unread(u32),
    FeedRevBumped,
    Media { cid: String, img: Result<egui::ColorImage, String> },
    Posted,
    // ── wallet ────────────────────────────────────────────────────────────────
    WalletAddresses { evm: String, ela: String, did: String, chains: Value },
    WalletBalance { chain: String, data: Value },
    WalletSent(Value),     // a completed tx record {chain,symbol,to,amount,hash,kind,ts}
    WalletSendFailed(String),
    WalletTxStatus { hash: String, status: String }, // EVM receipt poll → pending/success/failed
    WalletPhrase(String),  // the BIP39 recovery phrase, for the backup sheet reveal
    WalletLocked,          // identity has no BIP39 seed → show the create-seed panel
    // ── tipping ─────────────────────────────────────────────────────────────────
    TipResolved { did: String, addresses: Value }, // a recipient's published receive addresses
    TipTokens { did: String, tokens: Vec<Value> }, // ERC-20s the sender holds on ESC (asset picker)
    TipSent(Value),        // a completed tip tx record {chain,symbol,to,amount,hash,kind:"tip",ts}
    TipSendFailed { did: String, error: String },
    WalletSeedCreated,     // fresh BIP39 identity written → restart to load it
    OnboardRestored,       // identity restored from a phrase → restart to load it
    OnboardError(String),  // restore failed (e.g. invalid phrase)
    OnboardProfileSet(Result<Value, String>), // CREATE-new profile-setup submitted →
                           // finish onboarding either way (a failure only toasts, so
                           // the user is never trapped on the setup screen)
    Toast(String),
    Error(String),
    // ── calls (voice/video) ───────────────────────────────────────────────────
    CallSignals(Vec<Value>),   // batch from call_poll(); the UI drain dispatches each
    ContactTransport { did: String, transport: String }, // direct-gate probe result
}

/// 1:1 call lifecycle (mirrors Android's CallManager). `video` distinguishes a
/// video call from voice; `is_caller` decides who dials the media plane.
#[derive(Clone, PartialEq)]
pub enum CallState {
    Idle,
    Outgoing { peer: String, name: String, call_id: String, video: bool },
    Incoming { peer: String, name: String, call_id: String, video: bool },
    Active { peer: String, name: String, call_id: String, video: bool, is_caller: bool },
}

impl Default for CallState {
    fn default() -> Self {
        CallState::Idle
    }
}

impl CallState {
    pub fn peer(&self) -> Option<&str> {
        match self {
            CallState::Idle => None,
            CallState::Outgoing { peer, .. }
            | CallState::Incoming { peer, .. }
            | CallState::Active { peer, .. } => Some(peer),
        }
    }
    pub fn call_id(&self) -> Option<&str> {
        match self {
            CallState::Idle => None,
            CallState::Outgoing { call_id, .. }
            | CallState::Incoming { call_id, .. }
            | CallState::Active { call_id, .. } => Some(call_id),
        }
    }
}

/// Coerce a social::* JSON result into a Vec (the list APIs return a bare array,
/// or an `{"error":..}` object the UI treats as empty).
pub fn as_array(v: Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a,
        _ => Vec::new(),
    }
}
