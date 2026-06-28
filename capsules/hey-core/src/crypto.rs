// Hybrid post-quantum E2E encryption for DMs.
//
// Rust port of the reference JS pqcrypto implementation. Same
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
use ml_kem::{Ciphertext, EncodedSizeUser, KemCore, MlKem768, B32};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Pub, StaticSecret as X25519Priv};
use zeroize::Zeroizing;

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

/// Deterministically derive the ML-KEM-768 keypair from the 32-byte seed, so the
/// ENTIRE identity (did:key + X25519 + ML-KEM) reproduces from one seed — and
/// therefore from one BIP39 recovery phrase. FIPS 203 keygen is just
/// `generate_deterministic(d, z)`; we derive d and z from the seed with domain-
/// separated HKDF, so the result is a valid keypair, identical on every device
/// that holds the same seed. Returns `(decapsulation_key, encapsulation_key)`.
pub fn ml_kem_from_seed(seed: &[u8; 32]) -> (Vec<u8>, Vec<u8>) {
    let hk = Hkdf::<Sha256>::new(Some(b"hey-ml-kem-v1"), seed);
    let mut d = [0u8; 32];
    let mut z = [0u8; 32];
    hk.expand(b"ml-kem-d", &mut d).expect("hkdf d");
    hk.expand(b"ml-kem-z", &mut z).expect("hkdf z");
    let (dk, ek) = MlKem768::generate_deterministic(&B32::from(d), &B32::from(z));
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

// Returns the derived AEAD key in a `Zeroizing` wrapper so the local binding at
// each call site is wiped from the heap when it drops (L: transient AEAD key not
// zeroized). `Zeroizing<[u8;32]>` derefs to `[u8;32]`, so `&key` still coerces to
// the `&[u8;32]`/`&[u8]` the cipher constructors take — the derived bytes and
// every downstream output are byte-identical.
fn derive_key(x25519_secret: &[u8], kem_secret: &[u8]) -> Zeroizing<[u8; 32]> {
    let ikm = Zeroizing::new({
        let mut v = Vec::with_capacity(x25519_secret.len() + kem_secret.len());
        v.extend_from_slice(x25519_secret);
        v.extend_from_slice(kem_secret);
        v
    });
    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO, out.as_mut_slice()).expect("hkdf expand");
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
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&*key));
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

// ── At-rest encryption for the on-device store ───────────────────────────────
//
// Every persisted file (the BIP39 seed/identity, the Double-Ratchet PRIVATE keys
// dhs_priv/kem_priv, conversation plaintext, contacts, pinned peer keys) is
// sealed with this before it touches disk. `key` is the 32-byte storage DEK that
// the mobile runtime installs after the user unlocks — the DEK itself is wrapped
// by a hardware (StrongBox/TEE) Keystore key, so nothing here is readable at rest
// without the hardware key + the user's biometric/credential. ChaCha20-Poly1305
// (the same AEAD the DM seal uses) with a fresh random nonce per write.
//
// Format: MAGIC(7) || nonce(12) || ChaCha20-Poly1305(plaintext)  [tag appended].
// The magic lets `open_at_rest` tell an encrypted blob from a pre-encryption
// LEGACY PLAINTEXT file, so existing installs migrate transparently (read raw,
// re-encrypt on the next write) instead of looking corrupt.
const AT_REST_MAGIC: &[u8; 7] = b"HEYAR\x01\x00";

/// True if `blob` was produced by `seal_at_rest` (carries the magic header).
pub fn is_at_rest(blob: &[u8]) -> bool {
    blob.len() >= AT_REST_MAGIC.len() && &blob[..AT_REST_MAGIC.len()] == AT_REST_MAGIC
}

/// Seal `plaintext` for storage under the 32-byte DEK. Infallible (a panic here
/// would mean the AEAD impl itself failed, which never happens for valid keys).
pub fn seal_at_rest(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .expect("at-rest ChaCha20-Poly1305 encrypt");
    let mut out = Vec::with_capacity(AT_REST_MAGIC.len() + 12 + ct.len());
    out.extend_from_slice(AT_REST_MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    out
}

/// Open a blob produced by `seal_at_rest`. `None` if it has no magic (a legacy
/// plaintext file — caller should read it raw and re-encrypt) OR the AEAD tag
/// fails (wrong key / tampered — caller should treat as missing, never as data).
/// Use `is_at_rest` first to distinguish the two `None` cases.
pub fn open_at_rest(key: &[u8; 32], blob: &[u8]) -> Option<Vec<u8>> {
    if !is_at_rest(blob) || blob.len() < AT_REST_MAGIC.len() + 12 {
        return None;
    }
    let nonce = &blob[AT_REST_MAGIC.len()..AT_REST_MAGIC.len() + 12];
    let ct = &blob[AT_REST_MAGIC.len() + 12..];
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(key));
    cipher.decrypt(Nonce::from_slice(nonce), ct).ok()
}

// ── private FEED E2E (posts sealed to approved followers) ─────────────────────
// A post is published on the author's DID-derived gossip topic. Today it rides as
// signed CLEARTEXT, so any node that derives the topic can read it. These helpers
// make the feed PRIVATE: the author holds a per-account FEED KEY (derived from the
// identity seed + an EPOCH counter), seals every post under it (ChaCha20-Poly1305),
// and hands the current key to a follower only when it ACCEPTS that follower — over
// the existing sealed/ratcheted DM. Removing a follower bumps the epoch (a fresh key
// future posts use) and the new key is re-delivered to the remaining approved set, so
// a removed follower keeps only the old key and can't read new posts. The epoch is
// embedded in the sealed blob so a follower picks the right key from the ones it holds.
const FEED_POST_MAGIC: &[u8; 4] = b"HFP1";

/// The author's symmetric feed key for a given epoch. Deterministic from the identity
/// seed, so it survives reinstall/migration and every device of the same account derives
/// the same key; the epoch (bumped on follower removal) gives forward-secrecy on removal.
pub fn feed_key_from_seed(seed: &[u8; 32], epoch: u32) -> Zeroizing<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(b"hey-feed-key-v1"), seed);
    let mut out = Zeroizing::new([0u8; 32]);
    hk.expand(format!("feed-epoch:{epoch}").as_bytes(), out.as_mut_slice())
        .expect("hkdf feed key");
    out
}

/// Seal one post (the canonical post JSON) under the feed key for `epoch`. Returns a
/// base64 string `B64(MAGIC ‖ epoch_be ‖ nonce ‖ ct)` for direct embedding in the signed
/// feed event. A fresh random nonce per post + a per-account key ⇒ no nonce reuse.
pub fn seal_feed_post(feed_key: &[u8; 32], epoch: u32, plaintext: &[u8]) -> String {
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(feed_key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .expect("feed-post ChaCha20-Poly1305 encrypt");
    let mut out = Vec::with_capacity(4 + 4 + 12 + ct.len());
    out.extend_from_slice(FEED_POST_MAGIC);
    out.extend_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    B64.encode(out)
}

/// The epoch a sealed-feed blob was sealed under (so the reader can pick the right key
/// from the keys it holds for this author). `None` if the blob isn't a sealed feed post.
pub fn feed_post_epoch(sealed_b64: &str) -> Option<u32> {
    let blob = B64.decode(sealed_b64).ok()?;
    if blob.len() < 8 || &blob[0..4] != FEED_POST_MAGIC {
        return None;
    }
    Some(u32::from_be_bytes([blob[4], blob[5], blob[6], blob[7]]))
}

/// Open a sealed feed post with the feed key for its epoch. `None` if it's not a sealed
/// post, the epoch/key mismatch, or the AEAD tag fails (wrong key / tampered) — fail-closed,
/// never returns partial/plaintext.
pub fn open_feed_post(feed_key: &[u8; 32], sealed_b64: &str) -> Option<String> {
    let blob = B64.decode(sealed_b64).ok()?;
    if blob.len() < 4 + 4 + 12 || &blob[0..4] != FEED_POST_MAGIC {
        return None;
    }
    let nonce = &blob[8..20];
    let ct = &blob[20..];
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(feed_key));
    let pt = cipher.decrypt(Nonce::from_slice(nonce), ct).ok()?;
    String::from_utf8(pt).ok()
}

/// Collision-resistant originator tag embedded in a group `call_id` (`gc-{tag}-{ts}`).
/// Binds the call's media-secret + host to the FULL originator DID: a member is recognized
/// as the originator only if `gcall_origin_tag(their_did)` equals the tag carried in the
/// call_id. This replaces the prior 6-char did:key-TAIL match (~35 bits, grindable to spoof
/// the host / substitute the media secret) with a 96-bit pseudorandom commitment to the
/// whole DID — a second-preimage now costs ~2^96, infeasible. Hex so it never contains the
/// `-` separator the call_id parser splits on.
pub fn gcall_origin_tag(did: &str) -> String {
    let hk = Hkdf::<Sha256>::new(Some(b"hey-gcall-origin-v1"), did.as_bytes());
    let mut tag = [0u8; 12];
    hk.expand(b"gcall-origin", &mut tag).expect("hkdf gcall origin tag");
    tag.iter().map(|b| format!("{b:02x}")).collect()
}

// ── realtime-media E2E (1:1 calls) ───────────────────────────────────────────
// Voice/video frames are classical QUIC-TLS-only today. These add an APP-LAYER seal keyed
// off a fresh 32-byte per-call secret that rides INSIDE the sealed post-quantum DM call
// offer (so it inherits ML-KEM-768 + verified-identity binding). Both peers derive the SAME
// directional key pair + the SAME short-authentication-string (SAS); users compare the SAS
// out-of-band to rule out a MITM. Frames are ChaCha20-Poly1305 with an explicit per-frame
// counter as the nonce (a fresh per-call key + a strictly-monotonic counter ⇒ no nonce reuse).

/// Directional media keys from the shared call secret. Both peers derive the identical pair;
/// each uses one for TX and the other for RX (opposite roles), so the two directions never
/// share a key/nonce space.  (caller→peer, peer→caller)
pub fn media_keys(secret: &[u8; 32], call_id: &str, stream: &str) -> ([u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha256>::new(Some(call_id.as_bytes()), secret);
    let mut c2p = [0u8; 32];
    let mut p2c = [0u8; 32];
    // `stream` ("voice"/"video") DOMAIN-SEPARATES the keys so the two media streams NEVER share a
    // key — their independent per-frame counters then cannot collide into a reused ChaCha20 nonce.
    let info_c = format!("hey-media-{stream}-c2p-v1");
    let info_p = format!("hey-media-{stream}-p2c-v1");
    hk.expand(info_c.as_bytes(), &mut c2p).expect("hkdf media c2p");
    hk.expand(info_p.as_bytes(), &mut p2c).expect("hkdf media p2c");
    (c2p, p2c)
}

/// Short Authentication String — both peers derive the SAME 6 decimal digits from the shared
/// secret + call_id. Users read it to each other to confirm no man-in-the-middle.
pub fn media_sas(secret: &[u8; 32], call_id: &str) -> String {
    let hk = Hkdf::<Sha256>::new(Some(call_id.as_bytes()), secret);
    let mut out = [0u8; 4];
    hk.expand(b"hey-media-sas-v1", &mut out).expect("hkdf media sas");
    format!("{:06}", u32::from_be_bytes(out) % 1_000_000)
}

/// Seal one media frame → `[8-byte BE counter][ChaCha20-Poly1305(frame, nonce=00000000||counter)]`.
/// The caller MUST pass a strictly-monotonic per-key counter so the nonce never repeats.
pub fn media_seal(key: &[u8; 32], counter: u64, frame: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(key));
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), frame)
        .expect("media seal");
    let mut out = Vec::with_capacity(8 + ct.len());
    out.extend_from_slice(&counter.to_be_bytes());
    out.extend_from_slice(&ct);
    out
}

/// Open a sealed media frame → `(counter, plaintext)`, or `None` on a bad tag / short input.
/// Caller enforces replay/ordering using the returned counter.
pub fn media_open(key: &[u8; 32], wire: &[u8]) -> Option<(u64, Vec<u8>)> {
    if wire.len() < 8 + 16 {
        return None;
    }
    let counter = u64::from_be_bytes(wire[0..8].try_into().ok()?);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(key));
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    let pt = cipher.decrypt(Nonce::from_slice(&nonce), &wire[8..]).ok()?;
    Some((counter, pt))
}

// ── group media E2E ───────────────────────────────────────────────────────────
// A group call has N senders sharing ONE per-call key (every member derives the same one from the
// sealed call secret, so any member can open any other's frames). Per-SENDER nonce uniqueness comes
// from a 4-byte sender salt embedded in the nonce + a strictly-monotonic per-sender counter: distinct
// senders derive distinct salts (from their did:key) so (key, nonce) never collides ACROSS senders,
// and the monotonic counter prevents collisions WITHIN a sender. A non-member spliced onto the mesh
// never receives the sealed secret, so it cannot derive the key — it gets only ciphertext.

/// 32 fresh cryptographically-random bytes (OsRng) — e.g. a per-call group media secret.
pub fn random_secret() -> [u8; 32] {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    b
}

/// The single shared group-media key for a call (`stream` = "voice"/"video" domain-separates them).
pub fn media_group_key(secret: &[u8; 32], call_id: &str, stream: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(call_id.as_bytes()), secret);
    let mut k = [0u8; 32];
    let info = format!("hey-media-group-{stream}-v1");
    hk.expand(info.as_bytes(), &mut k).expect("hkdf group media key");
    k
}

/// A sender's 4-byte nonce salt — deterministic from the call secret + the sender's did:key, so it
/// is distinct per member and never collides with another member's salt under the same shared key.
pub fn media_group_salt(secret: &[u8; 32], call_id: &str, member_did: &str) -> [u8; 4] {
    let hk = Hkdf::<Sha256>::new(Some(call_id.as_bytes()), secret);
    let mut out = [0u8; 4];
    let info = format!("hey-media-group-salt-v1\0{member_did}");
    hk.expand(info.as_bytes(), &mut out).expect("hkdf group salt");
    out
}

/// Seal a group frame → `[4B salt][8B BE counter][ChaCha20-Poly1305(frame, nonce=salt||counter)]`.
/// `salt` MUST be the sender's [`media_group_salt`] and `counter` strictly-monotonic per sender.
pub fn media_group_seal(key: &[u8; 32], salt: [u8; 4], counter: u64, frame: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(key));
    let mut nonce = [0u8; 12];
    nonce[0..4].copy_from_slice(&salt);
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), frame)
        .expect("group media seal");
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&counter.to_be_bytes());
    out.extend_from_slice(&ct);
    out
}

/// Open a group frame → `(salt, counter, plaintext)`, or `None` on a bad tag / short input. The
/// receiver reads the salt+counter straight off the wire (no per-sender key map needed). `None` lets
/// the caller fall back to treating the bytes as plaintext (legacy/un-keyed sender) — graceful rollout.
pub fn media_group_open(key: &[u8; 32], wire: &[u8]) -> Option<([u8; 4], u64, Vec<u8>)> {
    if wire.len() < 12 + 16 {
        return None;
    }
    let mut salt = [0u8; 4];
    salt.copy_from_slice(&wire[0..4]);
    let counter = u64::from_be_bytes(wire[4..12].try_into().ok()?);
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(key));
    let mut nonce = [0u8; 12];
    nonce[0..4].copy_from_slice(&salt);
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    let pt = cipher.decrypt(Nonce::from_slice(&nonce), &wire[12..]).ok()?;
    Some((salt, counter, pt))
}

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
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&*key));
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
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&*key));
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

// ── Streaming (chunked) attachment crypto — HPC1 ────────────────────────────
// For BIG files (torrent-style): each segment is its OWN ChaCha20-Poly1305 unit,
// so sender + receiver process ONE segment at a time (O(chunk) RAM) instead of
// the whole file. `encrypt_attachment`/`decrypt_attachment` above stay the
// one-shot path for small/inline; HPC1 is a separate, additive format.
//
// SECURITY (load-bearing): confidentiality depends on a FRESH RANDOM key PER FILE
// (`begin_streamed_attachment`). The per-segment nonce = base_nonce XOR be64(index)
// is unique within a file ONLY because the key is never reused across files — NEVER
// reuse a (key, base_nonce) pair for two different files. The AAD binds
// (tag, base_nonce, index, total) so REORDER, TRUNCATION, and cross-file SPLICE are
// AEAD-rejected by construction, not merely improbable.

/// Magic marking an HPC1 streamed segment frame.
const ATT_CHUNK_MAGIC: &[u8; 4] = b"HPC1";
/// AAD domain tag for streamed segments (distinct from any one-shot path).
const ATT_CHUNK_AAD_TAG: u8 = 0xC1;
/// Plaintext bytes per streamed segment. The wire frame is MAGIC(4) || ct(+16B tag).
pub const ATT_SEG_PLAINTEXT_BYTES: usize = 256 * 1024;

/// Mint the per-file secrets for a streamed attachment: a FRESH random 32B key
/// (base64) + a FRESH random 12B base nonce. MUST be unique per file (module note).
pub fn begin_streamed_attachment() -> (String, [u8; 12]) {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    let mut base = [0u8; 12];
    OsRng.fill_bytes(&mut base);
    let key_b64 = B64.encode(key);
    key.fill(0);
    (key_b64, base)
}

/// Per-segment nonce = base_nonce with be64(index) XORed into its low 8 bytes.
fn chunk_nonce(base: &[u8; 12], index: u32) -> [u8; 12] {
    let mut n = *base;
    let ib = (index as u64).to_be_bytes();
    for i in 0..8 {
        n[4 + i] ^= ib[i];
    }
    n
}

/// AAD binding a segment to its file + position: tag || base_nonce || index || total.
fn chunk_aad(base: &[u8; 12], index: u32, total: u32) -> [u8; 21] {
    let mut a = [0u8; 21];
    a[0] = ATT_CHUNK_AAD_TAG;
    a[1..13].copy_from_slice(base);
    a[13..17].copy_from_slice(&index.to_be_bytes());
    a[17..21].copy_from_slice(&total.to_be_bytes());
    a
}

fn chunk_key(key_b64: &str) -> Result<[u8; 32], String> {
    let k = B64.decode(key_b64).map_err(|e| format!("chunk key b64: {e}"))?;
    <[u8; 32]>::try_from(k.as_slice()).map_err(|_| "chunk key must be 32 bytes".to_string())
}

/// Encrypt ONE plaintext segment → frame = MAGIC || ct(+tag). The whole file is
/// never resident: the caller streams segments through this.
pub fn encrypt_attachment_chunk(
    key_b64: &str,
    base_nonce: &[u8; 12],
    index: u32,
    total: u32,
    pt: &[u8],
) -> Result<Vec<u8>, String> {
    let key = chunk_key(key_b64)?;
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = chunk_nonce(base_nonce, index);
    let aad = chunk_aad(base_nonce, index, total);
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            chacha20poly1305::aead::Payload { msg: pt, aad: &aad },
        )
        .map_err(|e| format!("chunk encrypt: {e:?}"))?;
    let mut out = Vec::with_capacity(ATT_CHUNK_MAGIC.len() + ct.len());
    out.extend_from_slice(ATT_CHUNK_MAGIC);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt ONE HPC1 segment frame → plaintext. Auth-fails on reorder / truncation
/// (wrong total) / cross-file splice (the index, total, and base_nonce are bound
/// into the AAD).
pub fn decrypt_attachment_chunk(
    key_b64: &str,
    base_nonce: &[u8; 12],
    index: u32,
    total: u32,
    seg: &[u8],
) -> Result<Vec<u8>, String> {
    if seg.len() < ATT_CHUNK_MAGIC.len() + 16 {
        return Err("streamed segment too short".into());
    }
    if &seg[..ATT_CHUNK_MAGIC.len()] != ATT_CHUNK_MAGIC {
        return Err("streamed segment bad magic".into());
    }
    let ct = &seg[ATT_CHUNK_MAGIC.len()..];
    let key = chunk_key(key_b64)?;
    let cipher = ChaCha20Poly1305::new(ChachaKey::from_slice(&key));
    let nonce = chunk_nonce(base_nonce, index);
    let aad = chunk_aad(base_nonce, index, total);
    cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            chacha20poly1305::aead::Payload { msg: ct, aad: &aad },
        )
        .map_err(|e| format!("chunk decrypt (auth fail): {e:?}"))
}

#[cfg(test)]
mod chunk_tests {
    use super::*;

    #[test]
    fn roundtrip_multi_segment() {
        let (key, base) = begin_streamed_attachment();
        let total = 3u32;
        let segs: Vec<Vec<u8>> = vec![vec![1u8; 4096], vec![2u8; 4096], vec![9u8; 100]]; // short final
        let frames: Vec<Vec<u8>> = segs
            .iter()
            .enumerate()
            .map(|(i, s)| encrypt_attachment_chunk(&key, &base, i as u32, total, s).unwrap())
            .collect();
        for (i, (f, s)) in frames.iter().zip(segs.iter()).enumerate() {
            let pt = decrypt_attachment_chunk(&key, &base, i as u32, total, f).unwrap();
            assert_eq!(&pt, s);
        }
    }

    #[test]
    fn reorder_fails() {
        let (key, base) = begin_streamed_attachment();
        let f0 = encrypt_attachment_chunk(&key, &base, 0, 4, b"chunk-zero").unwrap();
        // decrypt frame 0 claiming index 2 → auth fail (index in AAD)
        assert!(decrypt_attachment_chunk(&key, &base, 2, 4, &f0).is_err());
    }

    #[test]
    fn truncation_total_mismatch_fails() {
        let (key, base) = begin_streamed_attachment();
        let f = encrypt_attachment_chunk(&key, &base, 0, 5, b"data").unwrap();
        // a different total → auth fail (total in AAD)
        assert!(decrypt_attachment_chunk(&key, &base, 0, 4, &f).is_err());
    }

    #[test]
    fn cross_file_splice_fails() {
        let (key_a, base_a) = begin_streamed_attachment();
        let (_key_b, base_b) = begin_streamed_attachment();
        let fa = encrypt_attachment_chunk(&key_a, &base_a, 0, 2, b"file-a-chunk").unwrap();
        // same key, a DIFFERENT base_nonce (cross-file) → AAD mismatch → fail
        assert!(decrypt_attachment_chunk(&key_a, &base_b, 0, 2, &fa).is_err());
    }

    #[test]
    fn old_client_oneshot_on_hpc1_fails_safe() {
        let (key, base) = begin_streamed_attachment();
        let f = encrypt_attachment_chunk(&key, &base, 0, 1, b"streamed-data").unwrap();
        // a legacy receiver runs ONE-SHOT decrypt_attachment on an HPC1 frame →
        // MUST auth-fail (never silently corrupt): magic≠HPA1 + AAD absent.
        assert!(decrypt_attachment(&f, &key).is_err());
    }
}

#[cfg(test)]
mod feed_gcall_tests {
    use super::*;

    #[test]
    fn feed_post_roundtrip_and_epoch_isolation() {
        let seed = [7u8; 32];
        let k0 = feed_key_from_seed(&seed, 0);
        let k1 = feed_key_from_seed(&seed, 1);
        assert_ne!(&k0[..], &k1[..], "different epochs must yield different keys");
        let sealed = seal_feed_post(&k0, 0, b"{\"caption\":\"hi\"}");
        assert_eq!(feed_post_epoch(&sealed), Some(0));
        // Correct key opens it.
        assert_eq!(open_feed_post(&k0, &sealed).as_deref(), Some("{\"caption\":\"hi\"}"));
        // A removed follower holding only the NEXT epoch's key cannot read epoch-0 posts.
        assert_eq!(open_feed_post(&k1, &sealed), None);
        // A wrong-seed account derives a different key → cannot read.
        let other = feed_key_from_seed(&[9u8; 32], 0);
        assert_eq!(open_feed_post(&other, &sealed), None);
    }

    #[test]
    fn feed_post_rejects_garbage_and_tamper() {
        let k = feed_key_from_seed(&[1u8; 32], 3);
        assert_eq!(feed_post_epoch("not-base64!!"), None);
        assert_eq!(open_feed_post(&k, "not-base64!!"), None);
        assert_eq!(feed_post_epoch(&B64.encode(b"short")), None);
        let mut sealed = seal_feed_post(&k, 3, b"secret").into_bytes();
        // flip a ciphertext byte → AEAD tag fails → None (never partial plaintext)
        let n = sealed.len();
        sealed[n - 2] ^= 0x01;
        let tampered = String::from_utf8(sealed).unwrap();
        assert_eq!(open_feed_post(&k, &tampered), None);
    }

    #[test]
    fn gcall_origin_tag_strong_binding() {
        let a = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
        let b = "did:key:z6MkfakeKEYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
        let ta = gcall_origin_tag(a);
        assert_eq!(ta.len(), 24, "96-bit tag = 24 hex chars");
        assert_eq!(ta, gcall_origin_tag(a), "deterministic for same DID");
        assert_ne!(ta, gcall_origin_tag(b), "different DID → different tag");
        // A DID sharing the old 6-char did:key tail must NOT collide on the new tag.
        let same_tail = format!("did:key:z6MkDIFFERENTbodysamesuffix{}", &a[a.len() - 6..]);
        assert_ne!(gcall_origin_tag(a), gcall_origin_tag(&same_tail));
        assert!(ta.bytes().all(|c| c.is_ascii_hexdigit()), "hex only, no '-' separator");
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn at_rest_round_trip_and_tamper() {
        let key = [7u8; 32];
        let plain = br#"{"mnemonic":"correct horse battery staple","dhs_priv":"..."}"#;
        let blob = seal_at_rest(&key, plain);
        assert!(is_at_rest(&blob), "sealed blob must carry the magic");
        assert_ne!(&blob[..], &plain[..], "must not be plaintext on disk");
        assert_eq!(open_at_rest(&key, &blob).as_deref(), Some(&plain[..]));

        // Wrong key fails the AEAD tag (no plaintext leak).
        assert_eq!(open_at_rest(&[9u8; 32], &blob), None);
        // A flipped ciphertext byte fails the tag.
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert_eq!(open_at_rest(&key, &tampered), None);
        // A legacy plaintext file has no magic → not treated as an at-rest blob.
        assert!(!is_at_rest(plain));
        assert_eq!(open_at_rest(&key, plain), None);
        // Fresh nonce each call → distinct ciphertext for the same input.
        assert_ne!(seal_at_rest(&key, plain), seal_at_rest(&key, plain));
    }

    // Pin INLINE_ATTACHMENT_MAX_BYTES (16000, in api/dms.rs) against the pad
    // ladder so a future tweak to either side can't silently spill an inline
    // attachment's sealed ciphertext into a bigger bucket. `pad_attachment`
    // prepends an 8-byte length prefix, so a B-byte plaintext seals
    // `att_bucket(8 + B)`. At B = 16000 the padded plaintext is 16008 bytes,
    // landing in the 16384 bucket; the largest payload that still fits that
    // bucket is 16376 (8 + 16376 == 16384), and one more byte (16377) spills to
    // the next bucket.
    #[test]
    fn att_bucket_inline_boundary() {
        assert_eq!(att_bucket(8 + 16000), 16384);
        assert_eq!(att_bucket(8 + 16376), 16384);
        assert!(att_bucket(8 + 16377) > 16384);
    }

    // encrypt_attachment -> base64(blob) over the wire -> base64 decode ->
    // decrypt_attachment must round-trip byte-for-byte at the inline size
    // boundaries (1 byte, the 16000 cap, and the 16376 payload that exactly
    // fills the 16384 bucket).
    #[test]
    fn encrypt_decrypt_attachment_inline_boundary() {
        for &n in &[1usize, 16000, 16376] {
            let data = vec![0x37u8; n];
            let (blob, key_b64) = encrypt_attachment(&data).expect("encrypt");
            // Simulate the inline wire hop: base64 the blob and decode it back.
            let wire = B64.encode(&blob);
            let blob_back = B64.decode(&wire).expect("wire b64 decode");
            let back = decrypt_attachment(&blob_back, &key_b64).expect("decrypt");
            assert_eq!(back.len(), n, "length mismatch at n={n}");
            assert_eq!(back, data, "round-trip mismatch at n={n}");
        }
    }
}
