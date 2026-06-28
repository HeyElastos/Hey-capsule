//! Local, self-sovereign identity — the `elastos://identity/*` provider, on-device.
//!
//! On the desktop/server runtime this scheme is "projected" from a wallet/Home.
//! On a phone, the phone IS the runtime, so it OWNS the key: a single 32-byte
//! Ed25519 seed (→ did:key + X25519 by hey-core's exact derivation) plus a
//! stored ML-KEM-768 keypair. That whole blob is what Android Keystore/StrongBox
//! wraps behind a fingerprint — see `from_blob`/`to_blob`. The crypto itself is
//! hey-core's, so the on-device identity is byte-identical to the browser/CLI one.
//!
//! Answers the wire hey-core::runtime::identity_provider speaks:
//!   whoami → {did_key}                       pubkeys → {x25519_pub_b64, ml_kem_pub_b64}
//!   sign {payload_b64} → {signature_hex}     x25519_dh {eph_pub_b64} → {shared_b64}
//!   ml_kem_decapsulate {ct_b64} → {shared_b64}   verify {...} → {valid}

use std::path::Path;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ed25519_compact::{KeyPair, Seed};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use hey_core::{crypto, identity as hid};

use crate::carrier::{err, ok};

/// The wrapped-at-rest form: exactly what Kotlin decrypts from Keystore and
/// hands back via `nativeStartWithIdentity`, and what we persist on first run.
#[derive(Serialize, Deserialize)]
pub struct IdentityBlob {
    /// BIP39 recovery phrase — the SINGLE root. did:key + X25519 + ML-KEM all
    /// derive deterministically from it, and the SAME phrase recovers the
    /// matching Elastos DID + wallets in official Essentials. Empty on legacy
    /// (pre-mnemonic) blobs, which fall back to the stored seed/ml_kem below.
    #[serde(default)]
    pub mnemonic: String,
    pub seed_b64: String,
    pub ml_kem_secret_b64: String,
    pub ml_kem_public_b64: String,
}

/// Persist an identity blob, sealed under the storage DEK when one is installed
/// (mobile, hardware-wrapped) so the seed/mnemonic/ML-KEM secret are ciphertext
/// at rest; plaintext only on the host/CLI where no DEK exists. Shared by
/// `load_or_create` and the bare-phrase restore path in lib.rs.
/// ATOMIC write for the account blob: a torn write of identity.json (truncate-then-write) on a
/// sealed-at-rest blob would be UNDECRYPTABLE = account loss. Write to a sibling .tmp then rename
/// (atomic on the same filesystem).
fn atomic_write(path: &Path, bytes: &[u8]) -> bool {
    let tmp = path.with_extension("heytmp");
    if std::fs::write(&tmp, bytes).is_err() {
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

pub(crate) fn write_identity_blob(path: &Path, blob: &IdentityBlob) {
    let json = serde_json::to_string_pretty(blob).unwrap_or_default();
    match hey_core::plat::seal_with_at_rest_key(json.as_bytes()) {
        Some(enc) => {
            let _ = atomic_write(path, &enc);
        }
        None => {
            let _ = atomic_write(path, json.as_bytes());
        }
    }
}

/// Read a (possibly sealed) identity blob, transparently migrating a legacy
/// plaintext `identity.json`. `None` if missing/corrupt/undecryptable.
pub(crate) fn read_identity_blob(path: &Path) -> Option<IdentityBlob> {
    let raw = std::fs::read(path).ok()?;
    let json = if hey_core::plat::at_rest_active() && crypto::is_at_rest(&raw) {
        String::from_utf8(hey_core::plat::open_with_at_rest_key(&raw)?).ok()?
    } else {
        String::from_utf8(raw).ok()?
    };
    serde_json::from_str(&json).ok()
}

// ── Carrier identity (headless-vault) ────────────────────────────────────────
//
// A small blob persisted under the SAME no-auth storage DEK as identity.json,
// holding ONLY the one-way-derived carrier node key + the public did:key — NEVER
// the seed / mnemonic / ML-KEM secret. It lets a vault-ON device cold-start the
// carrier HEADLESS (mesh + buffer sealed messages) without the biometric seed:
// blake3 is not invertible, so this blob reveals only the already-public node id
// + did. The seed stays sealed in the hardware vault; content decrypts on unlock.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct CarrierIdentity {
    pub carrier_sk_b64: String,
    pub did_key: String,
    #[serde(default)]
    pub v: u8,
}

impl CarrierIdentity {
    /// Decode the persisted carrier node key back to raw bytes (None if the blob
    /// is corrupt — the boot path treats that as fail-closed, never a fresh mint).
    pub(crate) fn carrier_sk(&self) -> Option<[u8; 32]> {
        B64.decode(self.carrier_sk_b64.trim()).ok()?.try_into().ok()
    }
}

/// Persist the carrier blob (sealed under the DEK on mobile). Returns whether the
/// write actually landed — callers that need atomicity (enableVault, before it
/// deletes identity.json) MUST check this, not assume success.
pub(crate) fn write_carrier_identity(path: &Path, ci: &CarrierIdentity) -> bool {
    let json = serde_json::to_string_pretty(ci).unwrap_or_default();
    // ATOMIC (temp+rename): a torn carrier-blob write would be undecryptable, and enableVault
    // deletes identity.json only after this returns true — a torn write here must report false.
    match hey_core::plat::seal_with_at_rest_key(json.as_bytes()) {
        Some(enc) => atomic_write(path, &enc),
        // host/CLI only (no DEK) — never plaintext on mobile (DEK always present).
        None => atomic_write(path, json.as_bytes()),
    }
}

pub(crate) fn read_carrier_identity(path: &Path) -> Option<CarrierIdentity> {
    let raw = std::fs::read(path).ok()?;
    let json = if hey_core::plat::at_rest_active() && crypto::is_at_rest(&raw) {
        String::from_utf8(hey_core::plat::open_with_at_rest_key(&raw)?).ok()?
    } else {
        String::from_utf8(raw).ok()?
    };
    serde_json::from_str(&json).ok()
}

impl Identity {
    /// The carrier node key — a ONE-WAY blake3 derivation of the seed (the exact
    /// value the carrier boots from). Persisting it lets a vault-ON device mesh
    /// headless WITHOUT exposing the seed (blake3 can't be inverted).
    pub(crate) fn carrier_sk_bytes(&self) -> [u8; 32] {
        blake3::derive_key("hey-carrier-node-key-v1", &self.seed)
    }
    pub(crate) fn to_carrier_identity(&self) -> CarrierIdentity {
        CarrierIdentity {
            carrier_sk_b64: B64.encode(self.carrier_sk_bytes()),
            did_key: self.did_key.clone(),
            v: 1,
        }
    }
}

pub struct Identity {
    seed: [u8; 32],
    mnemonic: Option<String>,
    did_key: String,
    x25519_priv: [u8; 32],
    x25519_pub: [u8; 32],
    ml_kem_secret: Vec<u8>,
    ml_kem_public: Vec<u8>,
}

/// Defense-in-depth: wipe the long-lived private key material from the heap when an
/// `Identity` drops, so the seed / X25519 private / ML-KEM secret (and the BIP39
/// phrase, which derives all of them) can't linger in freed pages. The at-rest copy
/// is already DEK-sealed on mobile; this clears the IN-MEMORY copy. did:key, the
/// public X25519/ML-KEM and the (already-public) derived carrier key are left as-is.
impl Drop for Identity {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.seed.zeroize();
        self.x25519_priv.zeroize();
        self.ml_kem_secret.zeroize();
        if let Some(m) = self.mnemonic.as_mut() {
            m.zeroize();
        }
    }
}

/// Derive Hey's 32-byte root seed from a BIP39 mnemonic (domain-separated from
/// the Elastos-wallet derivation that uses the same phrase — different trees,
/// one phrase). The mnemonic's standard 64-byte seed feeds a blake3 KDF.
fn seed_from_mnemonic(m: &bip39::Mnemonic) -> [u8; 32] {
    blake3::derive_key("hey-identity-seed-v1", &m.to_seed(""))
}

impl Identity {
    fn from_parts(seed: [u8; 32], mnemonic: Option<String>, ml_kem_secret: Vec<u8>, ml_kem_public: Vec<u8>) -> Self {
        let kp = KeyPair::from_seed(Seed::new(seed));
        // ed25519_compact::PublicKey derefs to [u8; 32]. (Avoid `.as_ref()`,
        // which is ambiguous once ml-kem's hybrid_array AsRef impl is in scope.)
        let pk: [u8; 32] = *kp.pk;
        let did_key = hid::public_key_to_did_key(&pk);
        let (x25519_priv, x25519_pub) = crypto::x25519_from_seed(&seed);
        Identity { seed, mnemonic, did_key, x25519_priv, x25519_pub, ml_kem_secret, ml_kem_public }
    }

    /// Fresh identity: a new BIP39 phrase → seed → deterministic ML-KEM. The
    /// phrase is the recoverable root (Essentials-compatible).
    pub fn generate() -> Self {
        let m = bip39::Mnemonic::generate(12).expect("bip39 generate");
        Self::from_mnemonic_inner(m)
    }

    /// Restore from a BIP39 recovery phrase (12/24 words).
    pub fn from_mnemonic(phrase: &str) -> Result<Self, String> {
        let m = bip39::Mnemonic::parse(phrase.trim()).map_err(|e| format!("bad recovery phrase: {e}"))?;
        Ok(Self::from_mnemonic_inner(m))
    }

    fn from_mnemonic_inner(m: bip39::Mnemonic) -> Self {
        let seed = seed_from_mnemonic(&m);
        let (sk, pk) = crypto::ml_kem_from_seed(&seed);
        Self::from_parts(seed, Some(m.to_string()), sk, pk)
    }

    /// Reconstitute from a Keystore-unlocked blob (the Android path). A blob with
    /// a mnemonic re-derives everything from it; a legacy blob uses its stored
    /// seed + ml_kem (so existing identities keep working).
    pub fn from_blob(blob: &IdentityBlob) -> Result<Self, String> {
        if !blob.mnemonic.trim().is_empty() {
            return Self::from_mnemonic(&blob.mnemonic);
        }
        let seed: [u8; 32] = B64
            .decode(&blob.seed_b64)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
            .ok_or("identity blob: bad seed")?;
        let sk = B64.decode(&blob.ml_kem_secret_b64).map_err(|e| format!("ml_kem secret: {e}"))?;
        let pk = B64.decode(&blob.ml_kem_public_b64).map_err(|e| format!("ml_kem public: {e}"))?;
        Ok(Self::from_parts(seed, None, sk, pk))
    }

    /// The BIP39 recovery phrase, if this identity has one (new identities do).
    pub fn mnemonic(&self) -> Option<&str> {
        self.mnemonic.as_deref()
    }

    pub fn to_blob(&self) -> IdentityBlob {
        IdentityBlob {
            mnemonic: self.mnemonic.clone().unwrap_or_default(),
            seed_b64: B64.encode(self.seed),
            ml_kem_secret_b64: B64.encode(&self.ml_kem_secret),
            ml_kem_public_b64: B64.encode(&self.ml_kem_public),
        }
    }

    /// Load `identity.json` from the data dir, else create + persist.
    ///
    /// On Android the file is sealed at rest under the storage DEK (a 32-byte key
    /// wrapped by a hardware StrongBox/TEE Keystore key, installed via
    /// `hey_set_storage_key` before this runs), so the seed / mnemonic / ML-KEM
    /// secret are NEVER plaintext on disk. A pre-encryption plaintext file is read
    /// and re-sealed on the next write (transparent migration). On the host/CLI no
    /// DEK is installed, so it stays plaintext exactly as before.
    pub fn load_or_create(dir: &Path) -> Self {
        let path = dir.join("identity.json");
        if let Some(blob) = read_identity_blob(&path) {
            if let Ok(id) = Self::from_blob(&blob) {
                // Migrate an UPGRADER's legacy plaintext identity.json to sealed-
                // at-rest the first time we run with a DEK installed. Storage-tree
                // files (ratchet/conv) re-seal on their next write; identity.json
                // rarely rewrites, so re-seal it here explicitly.
                if hey_core::plat::at_rest_active()
                    && !std::fs::read(&path).map(|b| crypto::is_at_rest(&b)).unwrap_or(true)
                {
                    write_identity_blob(&path, &blob);
                }
                return id;
            }
        }
        let id = Self::generate();
        let _ = std::fs::create_dir_all(dir);
        write_identity_blob(&path, &id.to_blob());
        id
    }

    /// The 32-byte root seed. EVERY key (did:key, X25519, ML-KEM, and the carrier
    /// node key) derives from this — so sealing the seed seals the whole node.
    pub fn seed(&self) -> [u8; 32] {
        self.seed
    }

    pub fn did_key(&self) -> &str {
        &self.did_key
    }

    /// Confined private-feed key for `epoch`: derived from the root seed via HKDF so the
    /// RAW SEED never leaves this module. The returned key is a one-way function of the seed
    /// (can't be inverted to recover it) and is epoch-scoped, so handing it to social.rs to
    /// seal/open posts exposes only that epoch's feed-read capability — never the node identity.
    pub fn feed_key(&self, epoch: u32) -> zeroize::Zeroizing<[u8; 32]> {
        crypto::feed_key_from_seed(&self.seed, epoch)
    }

    pub fn handle(&self, op: &str, req: &Value) -> Value {
        match op {
            "whoami" => ok(json!({ "did_key": self.did_key, "principal": self.did_key })),
            "pubkeys" => ok(json!({
                "x25519_pub_b64": B64.encode(self.x25519_pub),
                "ml_kem_pub_b64": B64.encode(&self.ml_kem_public),
            })),
            "sign" => match b64f(req, "payload_b64") {
                Some(payload) => ok(json!({ "signature_hex": hid::sign(&payload, &self.seed) })),
                None => err("sign: bad payload_b64"),
            },
            "x25519_dh" => match b64f(req, "eph_pub_b64").and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
                Some(eph) => ok(json!({ "shared_b64": B64.encode(crypto::dh(&self.x25519_priv, &eph)) })),
                None => err("x25519_dh: bad eph_pub_b64"),
            },
            "ml_kem_decapsulate" => match b64f(req, "ct_b64") {
                Some(ct) => match crypto::ml_kem_decapsulate_local(&ct, &self.ml_kem_secret) {
                    Ok(shared) => ok(json!({ "shared_b64": B64.encode(shared) })),
                    Err(e) => err(format!("ml_kem_decapsulate: {e}")),
                },
                None => err("ml_kem_decapsulate: bad ct_b64"),
            },
            "verify" => {
                let did = sf(req, "did_key");
                let payload = b64f(req, "payload_b64").unwrap_or_default();
                let sig = sf(req, "signature_hex");
                let valid = hid::did_key_to_public_key(&did)
                    .map(|pk| hid::verify(&payload, &sig, &pk))
                    .unwrap_or(false);
                ok(json!({ "valid": valid }))
            }
            other => err(format!("identity: unknown op {other}")),
        }
    }
}

fn sf(r: &Value, k: &str) -> String {
    r.get(k).and_then(Value::as_str).unwrap_or("").into()
}
fn b64f(r: &Value, k: &str) -> Option<Vec<u8>> {
    B64.decode(r.get(k)?.as_str()?).ok()
}
