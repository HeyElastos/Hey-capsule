// Profile API — Rust port of the storage-backed parts of
// capsules/hey-social/client/src/api/auth.js (profile read/write only;
// signup/signin live in passkey.rs).

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

pub async fn follow_user(peer_did: &str) -> Result<(), RuntimeError> {
    let me = ensure_profile().await?;
    if !peer_did.starts_with("did:key:z") {
        return Err(RuntimeError::new("Invalid did"));
    }
    if peer_did == me.did_key {
        return Err(RuntimeError::new("Cannot follow yourself"));
    }
    let _ = peer::join_topic(&format!("hey-v0/user/{peer_did}/posts")).await;
    let mut follows = read_follows().await;
    if !follows.following.contains(&peer_did.to_string()) {
        follows.following.push(peer_did.to_string());
    }
    write_follows(&follows).await?;
    let _ = sign_and_publish_follow(
        &format!("hey-v0/follow/{peer_did}"),
        "follow.request",
        json!({
            "target_did": peer_did,
            "from_name": me.name,
            "ts": now_ms(),
        }),
    )
    .await;
    Ok(())
}

pub async fn unfollow_user(peer_did: &str) -> Result<(), RuntimeError> {
    let _ = peer::leave_topic(&format!("hey-v0/user/{peer_did}/posts")).await;
    let mut follows = read_follows().await;
    follows.following.retain(|d| d != peer_did);
    write_follows(&follows).await?;
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
    // (removed) Shared-identity mirror — no more cross-sandbox writes
    // into .AppData/ElastOS/Identity/*. Once the identity-projection
    // provider exists, an explicit `identity.publish_display(...)`
    // op will be the supported way to share name/avatar/bio with
    // other capsules.
    Ok(me)
}
