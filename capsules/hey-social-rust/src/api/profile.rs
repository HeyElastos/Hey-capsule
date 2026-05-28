// Profile API — Rust port of the storage-backed parts of
// capsules/hey-social/client/src/api/auth.js (profile read/write only;
// signup/signin live in passkey.rs).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::runtime::{storage, RuntimeError};
use crate::session;
use crate::shell;

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

// Best-effort: hydrate the Hey-local profile, falling back to the shared
// identity (written by the home welcome flow / passkey sign-in) and
// synthesizing a minimal Hey record if needed.
pub async fn ensure_profile() -> Result<Profile, RuntimeError> {
    if let Some(v) = storage::read_json(PROFILE_FILE).await? {
        if let Ok(p) = serde_json::from_value::<Profile>(v.clone()) {
            // SECURITY backfill: pre-fix passkey signups (before db9ae38 in
            // the React reference) never wrote the shared identity, letting
            // a stranger overwrite the user via the home welcome wizard.
            // Mirror that one-shot migration.
            if let Ok(shared) = shell::read_shared_identity().await {
                let needs_backfill = shared
                    .as_ref()
                    .and_then(|s| s.get("didKey").and_then(|v| v.as_str()))
                    .map_or(true, |s| s.is_empty());
                if needs_backfill {
                    shell::write_shared_identity(&shell::build_profile(
                        &p.name,
                        &p.did_key,
                        &p.auth_key_hash,
                        "hey-backfill",
                    ))
                    .await;
                }
            }
            return Ok(p);
        }
    }
    // No Hey-local profile — synthesize from shared identity if present,
    // or from session.
    let shared = shell::read_shared_identity().await.ok().flatten();
    let session_user = session::current();

    let did_key = shared
        .as_ref()
        .and_then(|s| s.get("didKey").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| session_user.as_ref().map(|s| s.did_key.clone()))
        .ok_or_else(|| RuntimeError::new("Not signed in"))?;

    let name = shared
        .as_ref()
        .and_then(|s| s.get("name").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| session_user.as_ref().map(|s| s.name.clone()))
        .unwrap_or_else(|| "Hey user".into());

    let auth_key_hash = shared
        .as_ref()
        .and_then(|s| {
            s.get("recoveryKeyHash")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default();

    let mut me = Profile::new_with(&name, &did_key, &auth_key_hash);
    if let Some(s) = shared.as_ref() {
        if let Some(av) = s.get("avatar").and_then(|v| v.as_str()) {
            me.avatar = av.into();
        }
        if let Some(bio) = s.get("bio").and_then(|v| v.as_str()) {
            me.bio = bio.into();
        }
    }
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
    // Mirror the visible bits into the shared identity so the home shell
    // and other capsules pick up the changes without their own write.
    if let Some(mut shared) = shell::read_shared_identity().await.ok().flatten() {
        shared["name"] = json!(me.name);
        shared["avatar"] = json!(me.avatar);
        shared["bio"] = json!(me.bio);
        shell::write_shared_identity(&shared).await;
    }
    Ok(me)
}
