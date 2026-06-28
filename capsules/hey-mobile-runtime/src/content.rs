//! Content provider — the `elastos://content/*` scheme (media blobs).
//!
//! v1 is a LOCAL content-addressed store: publish hashes the bytes → a CID-like
//! key and writes them under the app data dir; fetch returns them by key; the
//! `/ipfs/<cid>` gateway serves them for direct <img>/<video> binding. This
//! makes same-device posting + media work and keeps the wire contract identical
//! to the runtime's content provider.
//!
//! FOLLOW-UP (cross-device media): back this with iroh-blobs so a CID published
//! on phone A is fetchable by phone B over the same carrier endpoint. The wire
//! shape here already matches, so that's a backing-store swap, not an API change.

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::carrier::{err, ok};

pub struct Content {
    root: PathBuf,
}

impl Content {
    pub fn new(dir: &Path) -> Self {
        let root = dir.join("content");
        let _ = std::fs::create_dir_all(&root);
        Content { root }
    }

    fn blob_path(&self, cid: &str) -> Option<PathBuf> {
        // CIDs are our own hashes (hex), so only [0-9a-z] — reject anything else.
        if cid.is_empty() || !cid.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        Some(self.root.join(cid))
    }

    pub fn get_bytes(&self, cid: &str) -> Option<Vec<u8>> {
        let raw = std::fs::read(self.blob_path(cid)?).ok()?;
        // AT-REST: media is DEK-sealed on disk (see publish). Decrypt on read when a DEK is
        // installed and the blob is in sealed form; a legacy plaintext blob (is_at_rest=false) or
        // host/CLI (no DEK) is returned as-is — backward compatible.
        if hey_core::plat::at_rest_active() && hey_core::crypto::is_at_rest(&raw) {
            hey_core::plat::open_with_at_rest_key(&raw)
        } else {
            Some(raw)
        }
    }

    pub fn handle(&self, op: &str, req: &Value) -> Value {
        match op {
            "publish" => {
                let data = match req.get("data").and_then(Value::as_str).and_then(|s| B64.decode(s).ok()) {
                    Some(d) => d,
                    None => return err("content.publish: missing/!b64 data"),
                };
                // Content address: "b" + blake3 hex of the PLAINTEXT. Stable + collision-safe.
                let cid = format!("b{}", blake3::hash(&data).to_hex());
                if let Some(p) = self.blob_path(&cid) {
                    // AT-REST: DEK-seal the media on disk so a stolen/rooted device or an Android
                    // backup can't read posted/feed/profile media in clear. The CID stays the
                    // plaintext hash (logical key); on read get_bytes decrypts. Atomic temp+rename
                    // so a torn write can't corrupt the store. No DEK (host/CLI) → plaintext.
                    let blob = hey_core::plat::seal_with_at_rest_key(&data).unwrap_or(data.clone());
                    let tmp = p.with_extension("heytmp");
                    if std::fs::write(&tmp, &blob).and_then(|_| std::fs::rename(&tmp, &p)).is_err() {
                        let _ = std::fs::remove_file(&tmp);
                        return err("content.publish: write failed");
                    }
                }
                log::info!("content.publish {} bytes -> {cid}", data.len());
                ok(json!({ "cid": cid }))
            }
            "fetch" => {
                let cid = req.get("cid").and_then(Value::as_str).unwrap_or("");
                match self.get_bytes(cid) {
                    Some(bytes) => ok(json!({ "data": B64.encode(bytes) })),
                    None => err(format!("content.fetch: no such cid {cid}")),
                }
            }
            "ensure" => ok(json!({ "ok": true })),
            "unpublish" => {
                if let Some(p) = req.get("cid").and_then(Value::as_str).and_then(|c| self.blob_path(c)) {
                    let _ = std::fs::remove_file(p);
                }
                ok(json!({ "ok": true }))
            }
            other => err(format!("content: unknown op {other}")),
        }
    }
}

/// Sniff a media content-type from magic bytes so the `/ipfs/<cid>` gateway can
/// hand the WebView a real `image/*` / `video/*` type — an `<img>`/`<video>`
/// won't render `application/octet-stream`.
pub fn sniff_mime(b: &[u8]) -> &'static str {
    match b {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ if b.len() > 11 && &b[4..8] == b"ftyp" => "video/mp4",
        [0x1A, 0x45, 0xDF, 0xA3, ..] => "video/webm",
        _ => "application/octet-stream",
    }
}
