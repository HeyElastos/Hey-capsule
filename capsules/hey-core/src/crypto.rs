// Hybrid post-quantum E2E encryption for DMs.
//
// Rust port of capsules/hey-chat/client/src/lib/pqcrypto.js. Same
// construction, byte-identical envelope shape, so a hey-chat client
// and a hey-social client can read each other's messages.
//
//   shared_secret = HKDF-SHA256(X25519_dh || ML-KEM-768_secret, info=HKDF_INFO)
//   ciphertext    = ChaCha20-Poly1305(plaintext, key=shared_secret, nonce)
//
// Why hybrid:
//   * ML-KEM-768 is the NIST FIPS 203 post-quantum KEM standard. The
//     RustCrypto ml-kem crate is the pure-Rust implementation.
//   * X25519 is the classical fallback. An attacker would have to break
//     BOTH primitives to recover plaintext. Same hybrid pattern Signal
//     PQXDH and the NIST PQ migration guidelines recommend.
//
// Single-shot per-message encryption — no key ratchet (the Double Ratchet
// is the planned fast-follow). Per-message FS via an ephemeral X25519
// keypair the sender generates and includes in the envelope.
//
// Wire format (every byte field base64-encoded in the JSON envelope):
//   { v: "hpq-1"|"hpq-2", eph: <32B>, kem: <1088B>, n: <12B>, ct: <varB> }
//
// hpq-2 adds fixed-size CONTENT PADDING: before sealing, the plaintext is
// length-prefixed (4B big-endian) and zero-padded up to the next size
// bucket, so the envelope's ciphertext length reveals only the bucket — not
// the real message size (SimpleX-style metadata hardening). hpq-1 envelopes
// (from older hey-social / the React messenger) are raw plaintext; we still
// DECRYPT them so no existing message becomes unreadable — only the version
// we ENCRYPT to moved to hpq-2.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key as ChachaKey, Nonce};
use hkdf::Hkdf;
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Ciphertext, EncodedSizeUser, KemCore, MlKem768};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Pub, StaticSecret as X25519Priv};

// HKDF domain separation stays "hpq-1" ACROSS envelope versions: padding
// changes only the plaintext, never key derivation, and changing this would
// break decryption of every existing hpq-1 envelope. Do NOT bump it with the
// envelope version.
const HKDF_INFO: &[u8] = b"hey-messenger/hpq-1";

/// Envelope version we ENCRYPT to. hpq-2 = fixed-size padded plaintext.
/// decrypt_hybrid still accepts hpq-1 (raw) for back-compat.
pub const ENVELOPE_VERSION: &str = "hpq-2";

/// Size buckets (bytes) the padded plaintext (incl. the 4-byte length
/// prefix) is rounded UP to. Anything larger rounds up to the next 64 KiB.
/// Buckets trade a little bandwidth for hiding the exact message length.
const PAD_BUCKETS: &[usize] = &[256, 1024, 4096, 16384, 65536];

/// Length-prefix (4B big-endian) + zero-pad `body` up to the next bucket.
fn pad_plaintext(body: &[u8]) -> Vec<u8> {
    let needed = 4 + body.len();
    let target = PAD_BUCKETS
        .iter()
        .copied()
        .find(|&b| b >= needed)
        .unwrap_or_else(|| needed.div_ceil(65536) * 65536);
    let mut out = Vec::with_capacity(target);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out.resize(target, 0);
    out
}

/// Inverse of `pad_plaintext`: read the length prefix, return the real bytes.
fn unpad_plaintext(padded: &[u8]) -> Result<Vec<u8>, String> {
    if padded.len() < 4 {
        return Err("padded plaintext shorter than length prefix".into());
    }
    let len = u32::from_be_bytes([padded[0], padded[1], padded[2], padded[3]]) as usize;
    if 4 + len > padded.len() {
        return Err("padding length prefix exceeds buffer".into());
    }
    Ok(padded[4..4 + len].to_vec())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpqEnvelope {
    pub v: String,
    pub eph: String, // base64 — 32B X25519 pub (ephemeral)
    pub kem: String, // base64 — ML-KEM-768 ciphertext (1088B)
    pub n: String,   // base64 — 12B nonce
    pub ct: String,  // base64 — ChaCha20-Poly1305 ciphertext + tag
}

/// Per-user persistent keypairs. The X25519 private is the user's
/// Ed25519 seed (we derive X25519 from the same 32 bytes — different
/// curve math, both stay strong). ML-KEM is generated fresh once and
/// persisted alongside the session.
#[derive(Debug, Clone)]
pub struct UserKeys {
    pub x25519_priv: [u8; 32],
    pub x25519_pub: [u8; 32],
    pub ml_kem_secret_bytes: Vec<u8>, // ~2400B
    pub ml_kem_public_bytes: Vec<u8>, // 1184B
}

/// Public projection — what we publish to peers via the profile bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKeys {
    pub x25519_pub_b64: String,
    pub ml_kem_pub_b64: String,
}

impl UserKeys {
    pub fn public(&self) -> PublicKeys {
        PublicKeys {
            x25519_pub_b64: B64.encode(self.x25519_pub),
            ml_kem_pub_b64: B64.encode(&self.ml_kem_public_bytes),
        }
    }
}

/// Derive an X25519 keypair from an Ed25519 seed. The X25519 pubkey is
/// independent of the Ed25519 pubkey (different curve math). Both can
/// be derived from the same 32-byte seed without weakening either.
pub fn x25519_from_seed(seed: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let priv_key = X25519Priv::from(*seed);
    let pub_key = X25519Pub::from(&priv_key);
    (*priv_key.as_bytes(), *pub_key.as_bytes())
}

/// Generate a fresh ML-KEM-768 keypair. Each user generates one at
/// first signin and persists it — the pubkey gets published via the
/// profile bundle.
pub fn generate_ml_kem_keypair() -> (Vec<u8>, Vec<u8>) {
    let mut rng = OsRng;
    let (dk, ek) = MlKem768::generate(&mut rng);
    (dk.as_bytes().to_vec(), ek.as_bytes().to_vec())
}

/// Build / load the full user-key bundle from an Ed25519 seed (hex auth_key).
pub fn keys_from_seed_and_kem(
    seed: &[u8; 32],
    ml_kem_secret: &[u8],
    ml_kem_public: &[u8],
) -> UserKeys {
    let (priv_bytes, pub_bytes) = x25519_from_seed(seed);
    UserKeys {
        x25519_priv: priv_bytes,
        x25519_pub: pub_bytes,
        ml_kem_secret_bytes: ml_kem_secret.to_vec(),
        ml_kem_public_bytes: ml_kem_public.to_vec(),
    }
}

fn derive_key(x25519_secret: &[u8], kem_secret: &[u8]) -> [u8; 32] {
    let mut ikm = Vec::with_capacity(x25519_secret.len() + kem_secret.len());
    ikm.extend_from_slice(x25519_secret);
    ikm.extend_from_slice(kem_secret);
    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut out = [0u8; 32];
    hk.expand(HKDF_INFO, &mut out).expect("hkdf expand");
    out
}

/// Encrypt to a recipient identified by their X25519 + ML-KEM-768 public
/// keys. Recipient must have previously published both pubkeys.
pub fn encrypt_to_hybrid(
    plaintext: &str,
    recipient_x25519_pub: &[u8; 32],
    recipient_kem_pub_bytes: &[u8],
) -> Result<HpqEnvelope, String> {
    // Ephemeral X25519 keypair — fresh per message for partial forward secrecy.
    let mut eph_seed = [0u8; 32];
    OsRng.fill_bytes(&mut eph_seed);
    let eph_priv = X25519Priv::from(eph_seed);
    eph_seed.fill(0);
    let eph_pub = X25519Pub::from(&eph_priv);
    let recipient_pub = X25519Pub::from(*recipient_x25519_pub);
    let x25519_secret = eph_priv.diffie_hellman(&recipient_pub);

    // ML-KEM-768 encapsulation against the recipient's KEM pubkey.
    let ek = <<MlKem768 as KemCore>::EncapsulationKey as EncodedSizeUser>::from_bytes(
        recipient_kem_pub_bytes
            .try_into()
            .map_err(|_| "ml-kem encapsulation key wrong size".to_string())?,
    );
    let (kem_ct, kem_secret) = ek
        .encapsulate(&mut OsRng)
        .map_err(|e| format!("ml-kem encapsulate: {e:?}"))?;

    let key = derive_key(x25519_secret.as_bytes(), &kem_secret);

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    // hpq-2: pad to a fixed bucket so ciphertext length leaks only the
    // bucket, not the true message size.
    let padded = pad_plaintext(plaintext.as_bytes());
    let ct = cipher
        .encrypt(nonce, padded.as_ref())
        .map_err(|e| format!("chacha encrypt: {e:?}"))?;

    let kem_bytes: &[u8] = kem_ct.as_slice();
    Ok(HpqEnvelope {
        v: ENVELOPE_VERSION.into(),
        eph: B64.encode(eph_pub.as_bytes()),
        kem: B64.encode(kem_bytes),
        n: B64.encode(nonce_bytes),
        ct: B64.encode(ct),
    })
}

// ── Double Ratchet primitives (FS + classical PCS) ───────────────────
//
// These are the pure key-schedule building blocks; the state machine that
// drives them lives in api/dms.rs. They DO NOT touch the frozen
// HKDF_INFO/derive_key path — the per-message AEAD key is still
// derive_key(x25519_half, kem_half); the ratchet only changes what the
// X25519-half IS (a chain-derived message key `mk`, not a raw DH output).
//
// SECURITY NOTE: classical X25519 + the DH ratchet below always deliver FS
// and PCS. The per-message ML-KEM encapsulation (retained in encrypt_with_mk)
// is to a STATIC key — harvest-now-decrypt-later confidentiality + the PQXDH
// root-key floor, NO FS/PCS by itself. PQ self-healing IS now implemented:
// `kdf_rk_hybrid` folds a fresh per-turn ML-KEM secret (from a rolling KEM
// keypair the ratchet rotates each turn — see api/dms.rs) into the root KDF, so
// for contacts bootstrapped after the hybrid upgrade, PCS is POST-QUANTUM
// (recovery after an unobserved turn needs breaking BOTH X25519 and ML-KEM-768).
// Pre-upgrade contacts stay classical-only via plain `kdf_rk`.

/// X25519 Diffie-Hellman: our private × their public → 32-byte shared.
pub fn dh(our_priv: &[u8; 32], their_pub: &[u8; 32]) -> [u8; 32] {
    let s = X25519Priv::from(*our_priv);
    let p = X25519Pub::from(*their_pub);
    *s.diffie_hellman(&p).as_bytes()
}

/// Generate a fresh ratchet X25519 keypair (private, public). A NEW one is
/// minted on every DH-ratchet send-turn; the old private MUST be discarded
/// (that discard is what delivers post-compromise security).
pub fn ratchet_keypair() -> ([u8; 32], [u8; 32]) {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let priv_k = X25519Priv::from(seed);
    seed.fill(0);
    let pub_k = X25519Pub::from(&priv_k);
    (priv_k.to_bytes(), *pub_k.as_bytes())
}

/// Initial root key (PQXDH-style hybrid floor): RK0 = HKDF(x3dh || kem_ss).
/// An attacker must break BOTH X25519 and ML-KEM-768 to recover RK0.
pub fn root_init(x3dh: &[u8], kem_ss: &[u8]) -> [u8; 32] {
    let mut ikm = Vec::with_capacity(x3dh.len() + kem_ss.len());
    ikm.extend_from_slice(x3dh);
    ikm.extend_from_slice(kem_ss);
    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut rk = [0u8; 32];
    hk.expand(b"hey-chat/ratchet/root-init/v1", &mut rk)
        .expect("hkdf root-init");
    rk
}

/// Root KDF on a DH-ratchet turn (Signal KDF_RK): salt=current RK, ikm=DH
/// output → (new root key, new chain key). The fresh DH output injects
/// entropy an attacker who saw old state didn't observe → PCS.
pub fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha256>::new(Some(rk), dh_out);
    let mut out = [0u8; 64];
    hk.expand(b"hey-chat/ratchet/root/v1", &mut out)
        .expect("hkdf root");
    let mut rk_new = [0u8; 32];
    let mut ck_new = [0u8; 32];
    rk_new.copy_from_slice(&out[..32]);
    ck_new.copy_from_slice(&out[32..]);
    (rk_new, ck_new)
}

/// Hybrid root KDF on a DH-ratchet turn: like `kdf_rk`, but the IKM is the
/// classical DH output CONCATENATED with a fresh per-turn ML-KEM shared secret.
/// Folding `kem_ss` (from a fresh encapsulation to the peer's ROLLING KEM key,
/// whose private is discarded each turn) makes post-compromise security
/// post-quantum: after a turn the attacker didn't observe, recovery needs
/// breaking BOTH X25519 and ML-KEM-768. Distinct domain string so a
/// hybrid-capable contact and a classical contact can never cross wires.
pub fn kdf_rk_hybrid(rk: &[u8; 32], dh_out: &[u8; 32], kem_ss: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut ikm = Vec::with_capacity(32 + kem_ss.len());
    ikm.extend_from_slice(dh_out);
    ikm.extend_from_slice(kem_ss);
    let hk = Hkdf::<Sha256>::new(Some(rk), &ikm);
    let mut out = [0u8; 64];
    hk.expand(b"hey-chat/ratchet/root-hybrid/v1", &mut out)
        .expect("hkdf root-hybrid");
    let mut rk_new = [0u8; 32];
    let mut ck_new = [0u8; 32];
    rk_new.copy_from_slice(&out[..32]);
    ck_new.copy_from_slice(&out[32..]);
    (rk_new, ck_new)
}

/// Chain KDF (Signal KDF_CK): one-way step → (message key, next chain key).
/// `ck` is treated as the HKDF PRK (already 32B uniform). Knowing ck_n
/// yields mk_n + ck_{n+1} but NOT ck_{n-1} (one-way ⇒ forward secrecy).
/// Caller MUST overwrite the old ck and delete mk right after use.
pub fn kdf_ck(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha256>::from_prk(ck).expect("ck is 32 bytes");
    let mut mk = [0u8; 32];
    let mut ck_next = [0u8; 32];
    hk.expand(b"hey-chat/ratchet/mk/v1", &mut mk)
        .expect("hkdf mk");
    hk.expand(b"hey-chat/ratchet/ck/v1", &mut ck_next)
        .expect("hkdf ck");
    (mk, ck_next)
}

/// Encrypt a ratchet message: the X25519-half is the chain message key
/// `mk` (NOT a per-message DH), and the envelope's `eph` field carries the
/// sender's CURRENT ratchet DH public key (so the receiver can advance its
/// DH ratchet). A fresh ML-KEM encapsulation to the recipient's static KEM
/// key still rides `kem`. Decrypt is `open_with_secrets(env, mk, kem_ss)`
/// where kem_ss is the recipient's decapsulation of `env.kem`.
pub fn encrypt_with_mk(
    plaintext: &str,
    mk: &[u8; 32],
    recipient_kem_pub_bytes: &[u8],
    ratchet_dh_pub: &[u8; 32],
) -> Result<HpqEnvelope, String> {
    let ek = <<MlKem768 as KemCore>::EncapsulationKey as EncodedSizeUser>::from_bytes(
        recipient_kem_pub_bytes
            .try_into()
            .map_err(|_| "ml-kem encapsulation key wrong size".to_string())?,
    );
    let (kem_ct, kem_secret) = ek
        .encapsulate(&mut OsRng)
        .map_err(|e| format!("ml-kem encapsulate: {e:?}"))?;
    let key = derive_key(mk, &kem_secret);

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let padded = pad_plaintext(plaintext.as_bytes());
    let ct = cipher
        .encrypt(nonce, padded.as_ref())
        .map_err(|e| format!("chacha encrypt: {e:?}"))?;

    Ok(HpqEnvelope {
        v: ENVELOPE_VERSION.into(),
        eph: B64.encode(ratchet_dh_pub), // ratchet DH pubkey, not a throwaway ephemeral
        kem: B64.encode(kem_ct.as_slice()),
        n: B64.encode(nonce_bytes),
        ct: B64.encode(ct),
    })
}

/// The X25519 ephemeral pubkey + ML-KEM ciphertext a recipient must feed to
/// the identity provider's `x25519_dh` / `ml_kem_decapsulate` ops. Pulled from
/// the envelope so the provider-backed decrypt path doesn't re-parse it.
pub fn envelope_recipient_inputs(env: &HpqEnvelope) -> Result<(Vec<u8>, Vec<u8>), String> {
    let eph = B64.decode(&env.eph).map_err(|e| format!("eph b64: {e}"))?;
    let kem_ct = B64.decode(&env.kem).map_err(|e| format!("kem b64: {e}"))?;
    Ok((eph, kem_ct))
}

/// Symmetric half of hybrid decrypt: given the two shared secrets (the X25519
/// DH output + the ML-KEM decapsulated secret), derive the AEAD key and open
/// the box. This lets a provider-backed recipient supply the shared secrets
/// (computed INSIDE the identity provider) without ever holding the private
/// keys. The local path (`decrypt_hybrid`) computes the same two secrets from
/// `UserKeys` and calls straight through here.
pub fn open_with_secrets(
    env: &HpqEnvelope,
    x25519_shared: &[u8],
    kem_shared: &[u8],
) -> Result<String, String> {
    let version = env.v.as_str();
    if version != "hpq-1" && version != "hpq-2" {
        return Err(format!("unsupported envelope version: {}", env.v));
    }
    let nonce_bytes: [u8; 12] = B64
        .decode(&env.n)
        .map_err(|e| format!("nonce b64: {e}"))?
        .try_into()
        .map_err(|_| "nonce wrong size".to_string())?;
    let ct = B64.decode(&env.ct).map_err(|e| format!("ct b64: {e}"))?;
    let key = derive_key(x25519_shared, kem_shared);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|e| format!("chacha decrypt (likely auth tag mismatch): {e:?}"))?;
    // hpq-2 plaintext is length-prefixed + padded; hpq-1 is raw.
    let body = if version == "hpq-2" {
        unpad_plaintext(&pt)?
    } else {
        pt
    };
    String::from_utf8(body).map_err(|e| format!("plaintext not utf-8: {e}"))
}

// ── Attachment encryption (E2E files) ────────────────────────────────
//
// Each attachment is sealed under its OWN fresh random ChaCha20-Poly1305 key.
// The CIPHERTEXT is uploaded to the (untrusted) blob/content store; the key
// rides inside the E2E-sealed DM, so the store/relay only ever holds opaque
// bytes. Fresh random key per file ⇒ identical files yield different ciphertext
// (no content-addressed dedup correlation). The blob layout is
// `ATT_PAD_MAGIC || nonce(12) || ct`, and the sealed plaintext is bucket-padded
// (length-prefixed) so the stored ciphertext length reveals only a bucket, not
// the real file size. Legacy blobs predate the magic + padding (`nonce || ct`)
// and still decrypt via the fallback branch in `decrypt_attachment`.

/// 4-byte magic marking the padded attachment blob format. Legacy blobs are
/// `nonce || ct` and (with overwhelming probability) never start with this, so
/// `decrypt_attachment` can tell the two apart; the AEAD is the final authority.
const ATT_PAD_MAGIC: &[u8; 4] = b"HPA1";

/// Bucket an attachment's padded length: reuse the message ladder (`PAD_BUCKETS`)
/// for ≤64 KiB (consistency with the message path), then switch to Padmé so a
/// 25 MiB upload pads by ≤~12% instead of the up-to-2× a power-of-two ladder
/// would cost across a 3-order-of-magnitude size range.
fn att_bucket(needed: usize) -> usize {
    PAD_BUCKETS
        .iter()
        .copied()
        .find(|&b| b >= needed)
        .unwrap_or_else(|| padme_bucket(needed))
}

/// Padmé padding (PURBs, Nikitin et al.): round `n` up so its binary form keeps
/// only ~log2(log2(n)) significant bits below the leading one — overhead bounded
/// at ~11%. Always returns a value ≥ `n`.
fn padme_bucket(n: usize) -> usize {
    if n < 2 {
        return n;
    }
    let e = (usize::BITS - 1 - n.leading_zeros()) as usize; // floor(log2 n)
    let s = (usize::BITS - 1 - (e as u32).leading_zeros()) as usize + 1; // floor(log2 e)+1
    let last_bits = e.saturating_sub(s);
    let mask = (1usize << last_bits) - 1;
    (n + mask) & !mask
}

/// Length-prefix (8-byte big-endian) + zero-pad attachment plaintext up to its
/// bucket. The u64 prefix (vs the message path's u32) keeps the format stable if
/// the 25 MiB cap is ever raised or chunking is added.
fn pad_attachment(body: &[u8]) -> Vec<u8> {
    let target = att_bucket(8 + body.len());
    let mut out = Vec::with_capacity(target);
    out.extend_from_slice(&(body.len() as u64).to_be_bytes());
    out.extend_from_slice(body);
    out.resize(target, 0);
    out
}

/// Inverse of `pad_attachment`: read the 8-byte length prefix, return the bytes.
fn unpad_attachment(padded: &[u8]) -> Result<Vec<u8>, String> {
    if padded.len() < 8 {
        return Err("padded attachment shorter than length prefix".into());
    }
    let mut lb = [0u8; 8];
    lb.copy_from_slice(&padded[..8]);
    let len = u64::from_be_bytes(lb) as usize;
    if 8 + len > padded.len() {
        return Err("attachment padding length prefix exceeds buffer".into());
    }
    Ok(padded[8..8 + len].to_vec())
}

/// Encrypt attachment bytes under a fresh key. Returns (blob, key_b64) where
/// blob = `ATT_PAD_MAGIC || nonce || ct(of bucket-padded plaintext)`. Only the
/// blob is uploaded; the key_b64 is sealed with the message.
pub fn encrypt_attachment(plaintext: &[u8]) -> Result<(Vec<u8>, String), String> {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    // Pad before sealing so the pad bytes are AEAD-authenticated (the store can't
    // strip padding without breaking the tag), mirroring the message path.
    let padded = pad_attachment(plaintext);
    let ct = cipher
        .encrypt(nonce, padded.as_ref())
        .map_err(|e| format!("attachment encrypt: {e:?}"))?;
    let mut out = Vec::with_capacity(ATT_PAD_MAGIC.len() + 12 + ct.len());
    out.extend_from_slice(ATT_PAD_MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    let key_b64 = B64.encode(key);
    key.fill(0); // zeroize the array copy; the b64 string is the durable carrier
    Ok((out, key_b64))
}

/// Inverse of `encrypt_attachment`. New blobs start with `ATT_PAD_MAGIC` and
/// carry bucket-padded plaintext; legacy blobs are `nonce || ct` and decrypt
/// raw via the fallback branch.
pub fn decrypt_attachment(blob: &[u8], key_b64: &str) -> Result<Vec<u8>, String> {
    let key = B64
        .decode(key_b64)
        .map_err(|e| format!("attachment key b64: {e}"))?;
    if key.len() != 32 {
        return Err("attachment key must be 32 bytes".into());
    }
    let padded_format =
        blob.len() >= ATT_PAD_MAGIC.len() && &blob[..ATT_PAD_MAGIC.len()] == ATT_PAD_MAGIC;
    let body = if padded_format {
        &blob[ATT_PAD_MAGIC.len()..]
    } else {
        blob
    };
    if body.len() < 12 + 16 {
        return Err("attachment ciphertext too short (nonce+tag)".into());
    }
    let (nonce_bytes, ct) = body.split_at(12);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("attachment decrypt (auth fail): {e:?}"))?;
    if padded_format {
        unpad_attachment(&pt)
    } else {
        Ok(pt)
    }
}

/// ML-KEM-768 encapsulation to a recipient's public key → (ciphertext, shared
/// secret). The KEM half of a hybrid seal, factored out so the Double Ratchet
/// bootstrap can encapsulate to a peer's STATIC KEM key without going through
/// the full ChaCha seal. `kem_ct` rides the wire; `kem_ss` feeds the key KDF.
pub fn ml_kem_encapsulate_local(kem_pub_bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let ek = <<MlKem768 as KemCore>::EncapsulationKey as EncodedSizeUser>::from_bytes(
        kem_pub_bytes
            .try_into()
            .map_err(|_| "ml-kem encapsulation key wrong size".to_string())?,
    );
    let (kem_ct, kem_ss) = ek
        .encapsulate(&mut OsRng)
        .map_err(|e| format!("ml-kem encapsulate: {e:?}"))?;
    Ok((kem_ct.as_slice().to_vec(), kem_ss.as_slice().to_vec()))
}

/// ML-KEM-768 decapsulation with our secret key → shared secret. The local
/// (seed/anon-holding) counterpart of the provider's `ml_kem_decapsulate`.
/// Used by both the single-shot decrypt and the ratchet's per-message KEM half.
pub fn ml_kem_decapsulate_local(kem_ct: &[u8], ml_kem_secret: &[u8]) -> Result<Vec<u8>, String> {
    let dk = <<MlKem768 as KemCore>::DecapsulationKey as EncodedSizeUser>::from_bytes(
        ml_kem_secret
            .try_into()
            .map_err(|_| "ml-kem decapsulation key wrong size".to_string())?,
    );
    let ct_arr = Ciphertext::<MlKem768>::try_from(kem_ct)
        .map_err(|_| "ml-kem ciphertext wrong size".to_string())?;
    let kem_ss = dk
        .decapsulate(&ct_arr)
        .map_err(|e| format!("ml-kem decapsulate: {e:?}"))?;
    Ok(kem_ss.as_slice().to_vec())
}

/// Decrypt an envelope using our X25519 private + ML-KEM secret (the local,
/// seed-holding path). Provider-backed recipients instead call the provider's
/// x25519_dh + ml_kem_decapsulate and feed the results to `open_with_secrets`.
pub fn decrypt_hybrid(env: &HpqEnvelope, keys: &UserKeys) -> Result<String, String> {
    let (eph_bytes, kem_ct) = envelope_recipient_inputs(env)?;
    let eph_pub_bytes: [u8; 32] = eph_bytes
        .try_into()
        .map_err(|_| "eph wrong size".to_string())?;
    let our_priv = X25519Priv::from(keys.x25519_priv);
    let eph_pub = X25519Pub::from(eph_pub_bytes);
    let x25519_secret = our_priv.diffie_hellman(&eph_pub);

    let dk = <<MlKem768 as KemCore>::DecapsulationKey as EncodedSizeUser>::from_bytes(
        keys.ml_kem_secret_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "ml-kem decapsulation key wrong size".to_string())?,
    );
    let kem_ct_arr: Ciphertext<MlKem768> = Ciphertext::<MlKem768>::clone_from_slice(&kem_ct);
    let kem_secret = dk
        .decapsulate(&kem_ct_arr)
        .map_err(|e| format!("ml-kem decapsulate: {e:?}"))?;

    open_with_secrets(env, x25519_secret.as_bytes(), &kem_secret)
}

/// Round-trip self-test. Run from a wasm debug console to sanity-check
/// the crypto stack:  `crypto::self_test()` should return `Ok(true)`.
pub fn self_test() -> Result<bool, String> {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let (priv_b, pub_b) = x25519_from_seed(&seed);
    let (kem_secret, kem_public) = generate_ml_kem_keypair();
    let keys = UserKeys {
        x25519_priv: priv_b,
        x25519_pub: pub_b,
        ml_kem_secret_bytes: kem_secret,
        ml_kem_public_bytes: kem_public,
    };
    let env = encrypt_to_hybrid(
        "hello, post-quantum world 🔒",
        &keys.x25519_pub,
        &keys.ml_kem_public_bytes,
    )?;
    let out = decrypt_hybrid(&env, &keys)?;
    if out != "hello, post-quantum world 🔒" {
        return Err(format!("self_test mismatch: {out}"));
    }

    // ── Double Ratchet primitives ────────────────────────────────────
    // root_init deterministic:
    let rk0 = root_init(b"x3dh-secret", b"kem-ss");
    if rk0 != root_init(b"x3dh-secret", b"kem-ss") {
        return Err("root_init nondeterministic".into());
    }
    // DH-ratchet root KDF advances the root:
    let (rk1, ck0) = kdf_rk(&rk0, &[9u8; 32]);
    if rk1 == rk0 {
        return Err("kdf_rk did not advance the root key".into());
    }
    // Hybrid root KDF is deterministic, advances the root, and folding a DIFFERENT
    // per-turn KEM secret yields a DIFFERENT root (so PQ-PCS actually depends on it):
    let (hrk, _hck) = kdf_rk_hybrid(&rk0, &[9u8; 32], b"kem-ss-a");
    if hrk != kdf_rk_hybrid(&rk0, &[9u8; 32], b"kem-ss-a").0 {
        return Err("kdf_rk_hybrid nondeterministic".into());
    }
    if hrk == rk0 || hrk == rk1 {
        return Err("kdf_rk_hybrid did not advance the root (or ignored the KEM secret)".into());
    }
    if hrk == kdf_rk_hybrid(&rk0, &[9u8; 32], b"kem-ss-b").0 {
        return Err("kdf_rk_hybrid ignored the per-turn KEM secret".into());
    }
    // Symmetric chain advances one-way; consecutive message keys differ
    // (the forward-secrecy property at the chain level):
    let (mk1, ck1) = kdf_ck(&ck0);
    let (mk2, ck2) = kdf_ck(&ck1);
    if mk1 == mk2 || ck1 == ck2 || ck0 == ck1 {
        return Err("kdf_ck chain not advancing (forward secrecy broken)".into());
    }
    // X25519 DH is symmetric across a fresh ratchet keypair:
    let (a_priv, a_pub) = ratchet_keypair();
    let (b_priv, b_pub) = ratchet_keypair();
    if dh(&a_priv, &b_pub) != dh(&b_priv, &a_pub) {
        return Err("x25519 ratchet DH not symmetric".into());
    }
    // mk-keyed envelope round-trips through the unchanged hpq path, with the
    // ratchet DH pubkey carried in `eph`:
    let renv = encrypt_with_mk("ratchet ping 🔐", &mk1, &keys.ml_kem_public_bytes, &a_pub)?;
    if B64.decode(&renv.eph).ok().as_deref() != Some(&a_pub[..]) {
        return Err("encrypt_with_mk: eph does not carry the ratchet DH pubkey".into());
    }
    let kem_ct = B64
        .decode(&renv.kem)
        .map_err(|e| format!("ratchet kem b64: {e}"))?;
    let dk = <<MlKem768 as KemCore>::DecapsulationKey as EncodedSizeUser>::from_bytes(
        keys.ml_kem_secret_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "ratchet dk size".to_string())?,
    );
    let ct_arr = Ciphertext::<MlKem768>::try_from(kem_ct.as_slice())
        .map_err(|_| "ratchet kem ct size".to_string())?;
    let kem_ss = dk
        .decapsulate(&ct_arr)
        .map_err(|e| format!("ratchet decapsulate: {e:?}"))?;
    let rout = open_with_secrets(&renv, &mk1, &kem_ss)?;
    if rout != "ratchet ping 🔐" {
        return Err(format!("ratchet envelope round-trip mismatch: {rout}"));
    }

    // ── Attachment padding ───────────────────────────────────────────
    // Round-trip across bucket boundaries (incl. the ladder→Padmé handoff):
    for &n in &[0usize, 1, 255, 257, 1000, 65536, 65537, 200_000] {
        let data: Vec<u8> = (0..n).map(|i| (i.wrapping_mul(31) + 7) as u8).collect();
        let (blob, k) = encrypt_attachment(&data)?;
        if !blob.starts_with(ATT_PAD_MAGIC) {
            return Err(format!("attachment blob missing pad magic at n={n}"));
        }
        let back = decrypt_attachment(&blob, &k)?;
        if back != data {
            return Err(format!("attachment round-trip mismatch at n={n}"));
        }
    }
    // Two different-size payloads in the SAME bucket → identical blob length
    // (the stored ciphertext reveals only the bucket, not the real size):
    let (b30a, _) = encrypt_attachment(&vec![1u8; 30_000])?;
    let (b30b, _) = encrypt_attachment(&vec![2u8; 31_000])?;
    if b30a.len() != b30b.len() {
        return Err("same-bucket attachments leak length via blob size".into());
    }
    // Padmé overhead bound (≤12%) on a 5 MiB payload:
    let five_mib = 5 * 1024 * 1024usize;
    if att_bucket(8 + five_mib) as f64 > (8 + five_mib) as f64 * 1.12 {
        return Err("padmé overhead exceeds 12% at 5 MiB".into());
    }
    // Legacy (unpadded `nonce||ct`) blobs still decrypt via the fallback path:
    {
        let mut lkey = [0u8; 32];
        OsRng.fill_bytes(&mut lkey);
        let mut nb = [0u8; 12];
        OsRng.fill_bytes(&mut nb);
        let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&lkey));
        let legacy_ct = cipher
            .encrypt(Nonce::from_slice(&nb), b"legacy attachment".as_ref())
            .map_err(|e| format!("legacy att enc: {e:?}"))?;
        let mut legacy_blob = Vec::with_capacity(12 + legacy_ct.len());
        legacy_blob.extend_from_slice(&nb);
        legacy_blob.extend_from_slice(&legacy_ct);
        let back = decrypt_attachment(&legacy_blob, &B64.encode(lkey))?;
        if back != b"legacy attachment" {
            return Err("legacy attachment decrypt failed".into());
        }
    }

    Ok(true)
}
