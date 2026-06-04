// Profile API — Rust port of the storage-backed parts of
// capsules/hey-social/client/src/api/auth.js (profile read/write only;
// signup/signin live in passkey.rs).

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::events::create_signed_event;
use crate::runtime::{ipfs, peer, storage, RuntimeError};
use crate::session;

pub const PROFILE_FILE: &str = "profile.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "authKeyHash")]
    pub auth_key_hash: String,
    #[serde(default, rename = "didKey")]
    pub did_key: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub followers: Vec<String>,
    #[serde(default)]
    pub following: Vec<String>,
    #[serde(default, rename = "pendingFollowers")]
    pub pending_followers: Vec<String>,
    #[serde(default, rename = "pendingFollowing")]
    pub pending_following: Vec<String>,
    #[serde(default, rename = "createdAt")]
    pub created_at: String,
    /// CID of the IPFS-pinned index of MY OWN posts. Followers pull this to
    /// backfill my full history (no IPNS/mutable pointer exists, so the head
    /// CID is advertised via events; this field is the latest value).
    #[serde(default, rename = "postsHead", skip_serializing_if = "Option::is_none")]
    pub posts_head: Option<String>,
}

impl Profile {
    pub fn new_with(name: &str, did_key: &str, auth_key_hash: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().chars().take(30).collect(),
            auth_key_hash: auth_key_hash.into(),
            did_key: did_key.into(),
            role: "general".into(),
            avatar: String::new(),
            bio: String::new(),
            followers: Vec::new(),
            following: Vec::new(),
            pending_followers: Vec::new(),
            pending_following: Vec::new(),
            created_at: js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default(),
            posts_head: None,
        }
    }
}

// Hydrate the Hey-local profile. Source of truth is the `did:key:z…`
// derived from the passkey PRF (in session.did_key) — that's the
// social federated identity. We deliberately do NOT consult any
// shared identity path or runtime principal here: the runtime
// principal (`person:local:…`) is a different ontology and would
// display as the user's DID if we let it.
//
// First-run path: no Hey/profile.json yet → seed one from session
// and PUT it. After that the file exists and reads cheaply. The 404
// on the first GET is expected and silent — storage::read_json
// returns Ok(None), no log, no banner.
pub async fn ensure_profile() -> Result<Profile, RuntimeError> {
    if let Some(v) = storage::read_json(PROFILE_FILE).await? {
        if let Ok(p) = serde_json::from_value::<Profile>(v) {
            return Ok(p);
        }
    }
    let session_user = session::current()
        .ok_or_else(|| RuntimeError::new("Not signed in"))?;
    let me = Profile::new_with(
        &session_user.name,
        &session_user.did_key,
        // We don't surface recovery-key state at the Hey level. The
        // value is kept on the Session record for the passkey-manager
        // modal; the profile itself doesn't need it.
        "",
    );
    let _ = storage::write_json(PROFILE_FILE, &serde_json::to_value(&me).unwrap_or(Value::Null))
        .await;
    Ok(me)
}

pub async fn read_profile() -> Result<Option<Profile>, RuntimeError> {
    match storage::read_json(PROFILE_FILE).await? {
        Some(v) => Ok(serde_json::from_value(v).ok()),
        None => Ok(None),
    }
}

pub async fn write_profile(p: &Profile) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(p).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(PROFILE_FILE, &v).await
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProfileUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

// ── Follows + avatar — mirrors capsules/hey-social/client/src/api/auth.js ──

const FOLLOWS_FILE: &str = "follows.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Follows {
    #[serde(default)]
    followers: Vec<String>,
    #[serde(default)]
    following: Vec<String>,
    #[serde(default)]
    pending: Vec<String>,
}

async fn read_follows() -> Follows {
    storage::read_json(FOLLOWS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

// Public projection of the follows store for the peer-receiver. Returns
// just the "following" list since that's what drives topic subscription.
pub async fn _internal_read_follows() -> FollowsPublic {
    let f = read_follows().await;
    FollowsPublic {
        followers: f.followers,
        following: f.following,
        pending: f.pending,
    }
}

pub struct FollowsPublic {
    pub followers: Vec<String>,
    pub following: Vec<String>,
    pub pending: Vec<String>,
}

async fn write_follows(f: &Follows) -> Result<(), RuntimeError> {
    let v = serde_json::to_value(f).map_err(|e| RuntimeError::new(format!("serialize: {e}")))?;
    storage::write_json(FOLLOWS_FILE, &v).await
}

/// Mirror follows.json (the source of truth) into profile.json's followers/
/// following arrays. profile.json is the doc that propagates to other capsules
/// and backs any follower/following count UI; left alone it stays stuck at the
/// empty arrays from signup. Cheap no-op when already in sync; called after
/// every follow-state mutation. Skips silently if there's no profile yet.
async fn sync_profile_follows() {
    let f = read_follows().await;
    if let Ok(Some(mut me)) = read_profile().await {
        if me.followers != f.followers || me.following != f.following {
            me.followers = f.followers;
            me.following = f.following;
            let _ = write_profile(&me).await;
        }
    }
}

async fn sign_and_publish_follow(
    topic: &str,
    event_type: &str,
    payload: Value,
) -> Result<(), RuntimeError> {
    session::current().ok_or_else(|| RuntimeError::new("Not signed in"))?;
    let evt = create_signed_event(event_type, payload)
        .await
        .map_err(|e| RuntimeError::new(format!("sign event: {e}")))?;
    let wire = crate::events::to_wire_string(&evt);
    peer::publish(peer::PublishArgs {
        topic,
        message: &wire,
        sender_id: &evt.sender_did,
        ts: evt.ts,
        signature: &evt.signature,
    })
    .await
    .map(|_| ())
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

// ── Peer node-ticket cache ───────────────────────────────────────────
// did:key -> iroh node ticket, learned from a hey-friend link. Lets the
// peer_receiver bootstrap the gossip mesh to a followed user's runtime on
// every poll (incl. after a restart), so their posts reach us cross-runtime.
const PEER_TICKETS_FILE: &str = "peer_tickets.json";

async fn read_peer_tickets() -> std::collections::HashMap<String, String> {
    storage::read_json(PEER_TICKETS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}
async fn write_peer_ticket(did: &str, ticket: &str) {
    if ticket.is_empty() {
        return;
    }
    let mut m = read_peer_tickets().await;
    m.insert(did.to_string(), ticket.to_string());
    if let Ok(v) = serde_json::to_value(&m) {
        let _ = storage::write_json(PEER_TICKETS_FILE, &v).await;
    }
}

/// Public projection for the peer_receiver: did -> node ticket, so followed
/// users' post topics can be joined with the right gossip bootstrap.
pub async fn peer_ticket_for(did: &str) -> Option<String> {
    read_peer_tickets().await.get(did).cloned()
}

// ── Posts-index head cache ───────────────────────────────────────────
// MY OWN head (the CID of my pinned posts index) lives on the profile so it
// propagates + survives restart. Followed users' heads (did -> head CID,
// learned from `posts.head` events) live here so the poll loop can re-pull a
// followed user's full history and fill any gap the live gossip event dropped.
const PEER_HEADS_FILE: &str = "peer_heads.json";

/// Store/refresh my own posts-index head CID on the profile.
pub async fn set_posts_head(cid: &str) {
    if let Ok(mut me) = ensure_profile().await {
        if me.posts_head.as_deref() != Some(cid) {
            me.posts_head = Some(cid.to_string());
            let _ = write_profile(&me).await;
        }
    }
}

/// My current posts-index head CID, if I've published one.
pub async fn my_posts_head() -> Option<String> {
    read_profile().await.ok().flatten().and_then(|p| p.posts_head)
}

async fn read_peer_heads() -> std::collections::HashMap<String, String> {
    storage::read_json(PEER_HEADS_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Record a followed user's latest posts-index head CID. Returns true if it
/// changed (so the caller knows a backfill is worth running).
pub async fn set_peer_head(did: &str, cid: &str) -> bool {
    let mut m = read_peer_heads().await;
    if m.get(did).map(|s| s.as_str()) == Some(cid) {
        return false;
    }
    m.insert(did.to_string(), cid.to_string());
    if let Ok(v) = serde_json::to_value(&m) {
        let _ = storage::write_json(PEER_HEADS_FILE, &v).await;
    }
    true
}

/// All known (followed-user did -> head CID) pairs, for the poll-time backfill.
pub async fn all_peer_heads() -> Vec<(String, String)> {
    read_peer_heads().await.into_iter().collect()
}

// ── Peer display-name cache (for the Following list) ─────────────────
// did -> human name, learned from the hey-friend link we followed with, or the
// from_name on an incoming follow.request. Best-effort: the Following list falls
// back to a short did when a name isn't known yet.
const PEER_NAMES_FILE: &str = "peer_names.json";

async fn read_peer_names() -> std::collections::HashMap<String, String> {
    storage::read_json(PEER_NAMES_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Cache a did's display name (idempotent; no-op for empty inputs).
pub async fn cache_peer_name(did: &str, name: &str) {
    if did.is_empty() || name.trim().is_empty() {
        return;
    }
    let mut m = read_peer_names().await;
    if m.get(did).map(|s| s.as_str()) == Some(name) {
        return;
    }
    m.insert(did.to_string(), name.to_string());
    if let Ok(v) = serde_json::to_value(&m) {
        let _ = storage::write_json(PEER_NAMES_FILE, &v).await;
    }
}

/// Best-known display name for a did (from the name cache), or empty.
pub async fn peer_name(did: &str) -> String {
    read_peer_names().await.get(did).cloned().unwrap_or_default()
}

// ── Public profile propagation (web2 parity: bio + counts on others' pages) ──
// We publish a small public profile doc (name/bio/avatar + follow counts) to
// IPFS and announce its head CID on our posts topic, exactly like the posts
// index. Followers fetch + cache it, so a remote profile page shows the real
// bio + counts instead of just what their posts reveal.
const PEER_PROFILES_FILE: &str = "peer_profiles.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PublicProfile {
    #[serde(default)]
    pub did: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub followers: usize,
    #[serde(default)]
    pub following: usize,
}

/// Pin my public profile doc to IPFS and return its head CID. Called on profile
/// edit + at boot so followers can fetch my current name/bio/avatar + counts.
pub async fn publish_own_profile() -> Option<String> {
    let me = read_profile().await.ok().flatten()?;
    let (following, followers) = follow_counts().await;
    let pp = PublicProfile {
        did: me.did_key.clone(),
        name: me.name,
        bio: me.bio,
        avatar: me.avatar,
        followers,
        following,
    };
    let bytes = serde_json::to_vec(&pp).ok()?;
    let resp = ipfs::add_bytes(&bytes, "public-profile.json", true).await.ok()?;
    ipfs::extract_cid(&resp)
}

/// Publish my profile + announce its head on my posts topic (followers drain it).
pub async fn publish_and_announce_profile() {
    let Some(head) = publish_own_profile().await else {
        return;
    };
    let Some(me_did) = session::current().map(|s| s.did_key) else {
        return;
    };
    let _ = sign_and_publish_follow(
        &format!("hey-v0/user/{me_did}/posts"),
        "profile.head",
        json!({ "head_cid": head }),
    )
    .await;
}

async fn read_peer_profiles() -> std::collections::HashMap<String, PublicProfile> {
    storage::read_json(PEER_PROFILES_FILE)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Cache a fetched public profile for a did (called by the profile.head handler).
pub async fn set_peer_profile(pp: PublicProfile) {
    if pp.did.is_empty() {
        return;
    }
    let mut m = read_peer_profiles().await;
    m.insert(pp.did.clone(), pp.clone());
    if let Ok(v) = serde_json::to_value(&m) {
        let _ = storage::write_json(PEER_PROFILES_FILE, &v).await;
    }
    // Keep the name cache in step so the Following list reads nicely too.
    cache_peer_name(&pp.did, &pp.name).await;
}

async fn get_peer_profile(did: &str) -> Option<PublicProfile> {
    read_peer_profiles().await.get(did).cloned()
}

/// Fetch + cache a peer's public profile from its head CID (profile.head event).
pub async fn fetch_peer_profile(did: &str, head_cid: &str) {
    let Ok(bytes) = ipfs::get_bytes(head_cid, None).await else {
        return;
    };
    if let Ok(mut pp) = serde_json::from_slice::<PublicProfile>(&bytes) {
        if pp.did.is_empty() {
            pp.did = did.to_string();
        }
        if pp.did == did {
            set_peer_profile(pp).await;
        }
    }
}

/// Build a best-effort profile view for ANOTHER user (we don't hold their full
/// profile doc): did + cached name + avatar/name learned from their posts. Lets
/// a follower's profile page render like a normal web2 profile instead of
/// hanging on "Loading". `posts` are their already-loaded posts.
pub async fn peer_profile_view(did: &str, posts: &[crate::api::posts::Post]) -> Profile {
    let from_post = posts.iter().find(|p| p.user_did == did);
    // Prefer the propagated public profile (real bio + counts); fall back to
    // name/avatar derived from their posts.
    let pub_profile = get_peer_profile(did).await;
    let cached = peer_name(did).await;
    let name = pub_profile
        .as_ref()
        .map(|p| p.name.clone())
        .filter(|n| !n.trim().is_empty())
        .or_else(|| Some(cached).filter(|n| !n.trim().is_empty()))
        .or_else(|| from_post.map(|p| p.user_name.clone()).filter(|n| !n.trim().is_empty()))
        .unwrap_or_else(|| short_label(did));
    let avatar = pub_profile
        .as_ref()
        .map(|p| p.avatar.clone())
        .filter(|a| !a.is_empty())
        .or_else(|| from_post.map(|p| p.user_avatar.clone()))
        .unwrap_or_default();
    let bio = pub_profile.as_ref().map(|p| p.bio.clone()).unwrap_or_default();
    // Encode counts as placeholder vecs (the UI reads .len()); we only have the
    // numbers, not the dids, for a remote user.
    let (followers, following) = pub_profile
        .as_ref()
        .map(|p| (vec![String::new(); p.followers], vec![String::new(); p.following]))
        .unwrap_or_default();
    Profile {
        id: String::new(),
        name,
        auth_key_hash: String::new(),
        did_key: did.to_string(),
        role: "general".into(),
        avatar,
        bio,
        followers,
        following,
        pending_followers: Vec::new(),
        pending_following: Vec::new(),
        created_at: String::new(),
        posts_head: None,
    }
}

fn short_label(did: &str) -> String {
    let s = did.strip_prefix("did:key:z").unwrap_or(did);
    let tail: String = s.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
    format!("hey-{tail}")
}

/// One entry in the Following list: the followed user's did + best-known name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowView {
    pub did: String,
    #[serde(default)]
    pub name: String,
}

fn with_names(dids: Vec<String>, names: &std::collections::HashMap<String, String>) -> Vec<FollowView> {
    dids.into_iter()
        .map(|did| {
            let name = names.get(&did).cloned().unwrap_or_default();
            FollowView { did, name }
        })
        .collect()
}

/// The people I follow (from follows.json), with cached display names where
/// known. Drives the Following tab.
pub async fn list_following() -> Vec<FollowView> {
    let f = read_follows().await;
    with_names(f.following, &read_peer_names().await)
}

/// The people who follow ME (from follows.json), with cached names. Followers
/// tab. record_follower caches their name from the follow.request from_name.
pub async fn list_followers() -> Vec<FollowView> {
    let f = read_follows().await;
    with_names(f.followers, &read_peer_names().await)
}

/// (following, followers) counts for the profile header / panel tabs.
pub async fn follow_counts() -> (usize, usize) {
    let f = read_follows().await;
    (f.following.len(), f.followers.len())
}

/// Record an incoming follower learned from a `follow.request`: cache their
/// node ticket, add them to our followers list, and bootstrap the gossip mesh
/// BACK to their runtime so the cross-runtime link is bidirectional (mirrors
/// the DM accept/handshake pattern in hey_core). Idempotent.
pub async fn record_follower(follower_did: &str, node_ticket: Option<&str>) {
    if !follower_did.starts_with("did:key:z") {
        return;
    }
    if let Some(t) = node_ticket.filter(|t| !t.is_empty()) {
        write_peer_ticket(follower_did, t).await;
    }
    let mut follows = read_follows().await;
    if !follows.followers.contains(&follower_did.to_string()) {
        follows.followers.push(follower_did.to_string());
        let _ = write_follows(&follows).await;
        sync_profile_follows().await;
    }
    // Connect back to their runtime (their follow inbox topic) so any later
    // interaction — follow-back, our posts reaching them — has a live overlay.
    let boot: Vec<String> = node_ticket
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .into_iter()
        .collect();
    let _ = peer::join_topic_with(&format!("hey-v0/follow/{follower_did}"), &boot).await;
    // Hand the new follower my current posts-index head over their follow inbox
    // so they can backfill my FULL history immediately (not just posts I make
    // from now on). Directed; rides the just-bootstrapped mesh.
    if let Some(head) = my_posts_head().await {
        let _ = sign_and_publish_follow(
            &format!("hey-v0/follow/{follower_did}"),
            "posts.head",
            json!({ "head_cid": head }),
        )
        .await;
    }
    // Also hand them my public profile head so my profile page renders fully
    // (bio + counts) for them right away.
    if let Some(phead) = publish_own_profile().await {
        let _ = sign_and_publish_follow(
            &format!("hey-v0/follow/{follower_did}"),
            "profile.head",
            json!({ "head_cid": phead }),
        )
        .await;
    }
}

/// Drop a follower learned from a `follow.unfollow`. Idempotent.
pub async fn remove_follower(follower_did: &str) {
    let mut follows = read_follows().await;
    let before = follows.followers.len() + follows.pending.len();
    follows.followers.retain(|d| d != follower_did);
    follows.pending.retain(|d| d != follower_did);
    if follows.followers.len() + follows.pending.len() != before {
        let _ = write_follows(&follows).await;
        sync_profile_follows().await;
    }
}

pub async fn follow_user(peer_did: &str) -> Result<(), RuntimeError> {
    follow_user_with(peer_did, None).await
}

/// Follow `peer_did`, bootstrapping the gossip mesh to their runtime via their
/// node ticket (from a hey-friend link) so the follow request — and their
/// posts — actually traverse separate runtimes. `None` ticket = same-runtime
/// only (a bare did, no node info).
pub async fn follow_user_with(peer_did: &str, node_ticket: Option<&str>) -> Result<(), RuntimeError> {
    let me = ensure_profile().await?;
    if !peer_did.starts_with("did:key:z") {
        return Err(RuntimeError::new("Invalid did"));
    }
    if peer_did == me.did_key {
        return Err(RuntimeError::new("Cannot follow yourself"));
    }
    let boot: Vec<String> = node_ticket
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .into_iter()
        .collect();
    if let Some(t) = node_ticket {
        write_peer_ticket(peer_did, t).await;
    }
    // Join their posts topic AND their follow inbox, bootstrapped to their node.
    let _ = peer::join_topic_with(&format!("hey-v0/user/{peer_did}/posts"), &boot).await;
    let _ = peer::join_topic_with(&format!("hey-v0/follow/{peer_did}"), &boot).await;
    let mut follows = read_follows().await;
    if !follows.following.contains(&peer_did.to_string()) {
        follows.following.push(peer_did.to_string());
    }
    write_follows(&follows).await?;
    sync_profile_follows().await;
    // Carry our own node ticket + DM pubkeys so the followee can connect BACK to
    // our runtime mesh AND bootstrap a DM channel to us (unified model: a
    // follower is message-able). Without the ticket the follow is one-way.
    let my_ticket = peer::my_ticket().await.unwrap_or_default();
    let my_keys = hey_core::api::dms::my_pubkeys().await;
    let _ = sign_and_publish_follow(
        &format!("hey-v0/follow/{peer_did}"),
        "follow.request",
        json!({
            "target_did": peer_did,
            "from_name": me.name,
            "from_ticket": my_ticket,
            "from_x25519": my_keys.as_ref().map(|k| k.x25519_pub_b64.clone()),
            "from_ml_kem": my_keys.as_ref().map(|k| k.ml_kem_pub_b64.clone()),
            "ts": now_ms(),
        }),
    )
    .await;
    Ok(())
}

// ── hey-friend link (did + node ticket) ──────────────────────────────
// The shareable identity token. Mirrors hey-invite: a bare did:key works too
// (same-runtime only) but a hey-friend link also carries the node ticket so a
// follow forms the cross-runtime mesh.
// v2 carries DM pubkeys so a follow can bootstrap a DM-capable contact (unified
// Following = people you can message). v1 links still decode (follow-only).
const FRIEND_LINK_VERSION: u8 = 2;

#[derive(Serialize, Deserialize)]
struct FriendLink {
    v: u8,
    did: String,
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node_ticket: Option<String>,
    /// X25519 + ML-KEM DM pubkeys (v2+). Let the follower bootstrap an encrypted
    /// DM channel without a separate invite handshake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x25519_pub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ml_kem_pub: Option<String>,
}

/// Our shareable hey-friend link: did + node ticket + name + DM pubkeys.
pub async fn my_friend_link() -> Result<String, RuntimeError> {
    let me = ensure_profile().await?;
    let keys = hey_core::api::dms::my_pubkeys().await;
    let link = FriendLink {
        v: FRIEND_LINK_VERSION,
        did: me.did_key.clone(),
        name: me.name.clone(),
        node_ticket: peer::my_ticket().await,
        x25519_pub: keys.as_ref().map(|k| k.x25519_pub_b64.clone()),
        ml_kem_pub: keys.as_ref().map(|k| k.ml_kem_pub_b64.clone()),
    };
    let bytes = serde_json::to_vec(&link).map_err(|e| RuntimeError::new(format!("friend link: {e}")))?;
    Ok(format!("hey-friend:{}", B64.encode(bytes)))
}

fn decode_friend_link(token: &str) -> Result<FriendLink, String> {
    let t = token.trim();
    // A bare did:key is accepted (same-runtime only — no ticket/keys).
    if t.starts_with("did:key:z") {
        return Ok(FriendLink {
            v: FRIEND_LINK_VERSION,
            did: t.to_string(),
            name: String::new(),
            node_ticket: None,
            x25519_pub: None,
            ml_kem_pub: None,
        });
    }
    let b64 = t
        .strip_prefix("hey-friend:")
        .ok_or("not a hey-friend link or did:key")?;
    let bytes = B64.decode(b64.trim()).map_err(|e| format!("friend link base64: {e}"))?;
    let link: FriendLink =
        serde_json::from_slice(&bytes).map_err(|e| format!("friend link json: {e}"))?;
    if !link.did.starts_with("did:key:z") {
        return Err("friend link did is not a did:key".into());
    }
    Ok(link)
}

/// Add a friend from a pasted hey-friend link OR a bare did:key. Returns the
/// followed did on success.
pub async fn follow_link(token: &str) -> Result<String, RuntimeError> {
    let link = decode_friend_link(token).map_err(RuntimeError::new)?;
    follow_user_with(&link.did, link.node_ticket.as_deref()).await?;
    // Remember their name (from the link) so the Following list reads nicely.
    cache_peer_name(&link.did, &link.name).await;
    // Unified model: if the link carries DM pubkeys (v2+), bootstrap an encrypted
    // DM-capable contact so "Message" works without a separate invite. Follow-only
    // for a bare did / v1 link (no keys).
    if let (Some(x), Some(k)) = (link.x25519_pub.clone(), link.ml_kem_pub.clone()) {
        let keys = hey_core::api::dms::PeerKeys {
            x25519_pub_b64: x,
            ml_kem_pub_b64: k,
        };
        // verified=true: the friend link is a direct, self-asserted key exchange.
        let _ = hey_core::api::dms::bootstrap_contact_from_keys(
            &link.did,
            &link.name,
            keys,
            link.node_ticket.clone(),
            true,
        )
        .await;
    }
    Ok(link.did)
}

pub async fn unfollow_user(peer_did: &str) -> Result<(), RuntimeError> {
    let _ = peer::leave_topic(&format!("hey-v0/user/{peer_did}/posts")).await;
    let mut follows = read_follows().await;
    follows.following.retain(|d| d != peer_did);
    write_follows(&follows).await?;
    sync_profile_follows().await;
    let _ = sign_and_publish_follow(
        &format!("hey-v0/follow/{peer_did}"),
        "follow.unfollow",
        json!({ "target_did": peer_did, "ts": now_ms() }),
    )
    .await;
    Ok(())
}

pub async fn is_following(peer_did: &str) -> bool {
    read_follows().await.following.iter().any(|d| d == peer_did)
}

// Avatar upload: pick a file → IPFS pin → set profile.avatar to the
// gateway URL → dual-write shared identity so other capsules pick up
// the new avatar without their own write. Returns the new gateway URL.
pub async fn upload_avatar(
    bytes: &[u8],
    filename: &str,
    mime: &str,
) -> Result<Profile, RuntimeError> {
    // Shrink in-browser (resize + WebP) before upload so a large photo doesn't
    // 413 against the runtime's provider body limit — same path posts use.
    let data = match crate::media::compress_image(bytes, mime).await {
        Some((b, _)) => b,
        None => bytes.to_vec(),
    };
    let resp = ipfs::add_bytes(&data, filename, true).await?;
    let cid = ipfs::extract_cid(&resp)
        .ok_or_else(|| RuntimeError::new("content.publish returned no cid"))?;
    let url = crate::runtime::ipfs::gateway_url(&cid, None);
    update_profile(ProfileUpdate {
        avatar: Some(url),
        ..Default::default()
    })
    .await
}

pub async fn update_profile(patch: ProfileUpdate) -> Result<Profile, RuntimeError> {
    let mut me = ensure_profile().await?;
    if let Some(n) = patch.name {
        me.name = n.trim().chars().take(30).collect();
    }
    if let Some(b) = patch.bio {
        me.bio = b.chars().take(280).collect();
    }
    if let Some(a) = patch.avatar {
        me.avatar = a;
    }
    write_profile(&me).await?;
    // Propagate the updated public profile (name/bio/avatar + counts) so
    // followers see it on my profile page — web2 parity.
    publish_and_announce_profile().await;
    Ok(me)
}
