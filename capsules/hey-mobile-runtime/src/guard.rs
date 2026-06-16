//! guard.rs — the law layer of the mini-runtime.
//!
//! The full elastOS runtime enforces a constitution: authority must be named,
//! narrow, revocable, and auditable; secrets may be used but never owned;
//! failure must be loud and closed. On the desktop/server runtime that law
//! lives in the capability plane (manifest `authority`, capability tokens,
//! provider allowlists, audit events). The phone IS the runtime here, so the
//! same law must live in-process — this module is it:
//!
//!   * `check(scheme, op)`   — named + narrow: the provider plane only answers
//!     ops on the declared capability table; anything else is denied loudly
//!     and recorded, never silently obeyed.
//!   * `authorize_spend` / `redeem_spend` — money moves only under a one-shot
//!     grant bound to (kind, recipient, amount) with a short TTL. The UI's
//!     confirmation produces the grant; the signer consumes it. Revocation is
//!     structural: single use + expiry + `revoke_spends()`.
//!   * `audit(...)`          — append-only on-device record of privileged acts
//!     (transfers, grants, denials). No secrets ever enter the log.
//!
//! Honest threat model: everything here is ONE process. A fully compromised
//! process can call anything — the OS app sandbox is the outer wall. What this
//! layer buys inside that wall: (1) no code path reaches the signer without
//! the user-confirmation sequence minting a grant first; (2) every privileged
//! act leaves a record the user can read; (3) a bug that wanders off its
//! declared authority is stopped and logged instead of obeyed. Binding grants
//! to a hardware-gated Keystore signature (BiometricPrompt CryptoObject
//! verified here) is the designed next rung on the same ladder.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

// ── capability law: named, narrow ────────────────────────────────────────────

/// The provider-plane authority this runtime grants its app — the mobile
/// mirror of the capsule's `boot_capabilities` plus the identity/blob surface
/// the engine itself needs. One source of truth; dispatch enforces it.
const CAPABILITIES: &[(&str, &[&str])] = &[
    (
        "identity",
        &["whoami", "pubkeys", "sign", "x25519_dh", "ml_kem_decapsulate", "verify"],
    ),
    (
        "peer",
        &[
            "init",
            "connect",
            "get_config",
            "get_ticket",
            "my_ticket",
            "gossip_join",
            "gossip_join_peers",
            "gossip_leave",
            "gossip_send",
            "gossip_recv",
            "list_topic_peers",
            "list_peers",
            "peer_paths",
        ],
    ),
    ("blobs", &["init", "add_bytes", "fetch", "share", "list", "drop"]),
    ("content", &["publish", "fetch", "ensure", "unpublish"]),
    ("ipfs", &["publish", "fetch", "ensure", "unpublish"]),
    ("did", &["resolve"]),
];

/// True if this runtime serves the scheme at all. Unknown schemes stay the
/// caller's graceful-404 path (hey-core falls back), exactly like before.
pub fn known_scheme(scheme: &str) -> bool {
    CAPABILITIES.iter().any(|(s, _)| *s == scheme)
}

/// Fail-closed op gate for a known scheme. A denial is loud (named error) and
/// audited — the constitutional opposite of pattern-matching into a handler's
/// default arm.
pub fn check(scheme: &str, op: &str) -> Result<(), String> {
    let ops = CAPABILITIES
        .iter()
        .find(|(s, _)| *s == scheme)
        .map(|(_, ops)| *ops)
        .unwrap_or(&[]);
    if ops.contains(&op) {
        return Ok(());
    }
    audit("capability.deny", json!({ "scheme": scheme, "op": op }));
    Err(format!(
        "capability denied: op '{op}' is outside the granted authority for elastos://{scheme}/*"
    ))
}

// ── audit: append-only record of privileged acts ─────────────────────────────

static AUDIT: OnceLock<Mutex<AuditLog>> = OnceLock::new();

struct AuditLog {
    path: Option<PathBuf>,
    /// In-memory tail so the viewer works even before/without a data dir.
    tail: VecDeque<String>,
}

fn audit_log() -> &'static Mutex<AuditLog> {
    AUDIT.get_or_init(|| Mutex::new(AuditLog { path: None, tail: VecDeque::new() }))
}

/// Point the audit log at the runtime data dir. Idempotent; called from init.
pub fn init(data_dir: &Path) {
    if let Ok(mut a) = audit_log().lock() {
        if a.path.is_none() {
            a.path = Some(data_dir.join("audit.jsonl"));
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const AUDIT_ROTATE_BYTES: u64 = 512 * 1024;
const AUDIT_TAIL_CAP: usize = 400;

/// Record a privileged act. `detail` must never contain key material — names,
/// amounts, recipients, outcomes only. Failure to persist is itself loud (it
/// lands in the log ring), but never blocks the act being recorded in memory.
pub fn audit(event: &str, detail: Value) {
    let line = json!({ "ts": now_ms(), "event": event, "detail": detail }).to_string();
    // Logcat: EVENT NAME ONLY — never the detail. The detail carries wallet metadata
    // (spend recipients/amounts, tx hashes, contact DIDs); logcat is adb/forensic-
    // reachable, so the financial trail must not land there in cleartext.
    log::info!("audit event={event}");
    let Ok(mut a) = audit_log().lock() else { return };
    a.tail.push_back(line.clone());
    while a.tail.len() > AUDIT_TAIL_CAP {
        a.tail.pop_front();
    }
    if let Some(path) = a.path.clone() {
        // SEALED at rest (whole-file ChaCha20-Poly1305): confidentiality for the
        // financial metadata AND tamper-evidence — any edit or truncation breaks the
        // auth tag, so a compromised component can't silently rewrite its own trail.
        // Read-modify-write (the log is small + bounded). Plaintext ONLY when no DEK
        // exists (CLI/host harness); a legacy plaintext file is migrated on this write.
        let mut content: Vec<u8> = match std::fs::read(&path) {
            Ok(b) if hey_core::crypto::is_at_rest(&b) => {
                hey_core::plat::open_with_at_rest_key(&b).unwrap_or_default()
            }
            Ok(b) => b,
            Err(_) => Vec::new(),
        };
        // Rotate: keep one prior generation so the record stays bounded.
        if content.len() as u64 > AUDIT_ROTATE_BYTES {
            let _ = std::fs::rename(&path, path.with_extension("jsonl.1"));
            content.clear();
        }
        content.extend_from_slice(line.as_bytes());
        content.push(b'\n');
        let on_disk = hey_core::plat::seal_with_at_rest_key(&content).unwrap_or(content);
        if let Err(e) = std::fs::write(&path, on_disk) {
            log::error!("audit persist failed: {e}");
        }
    }
}

/// Most recent audit lines (newest last) for the on-device viewer.
pub fn audit_tail(limit: usize) -> Vec<String> {
    audit_log()
        .lock()
        .map(|a| a.tail.iter().rev().take(limit).cloned().collect::<Vec<_>>())
        .map(|mut v| {
            v.reverse();
            v
        })
        .unwrap_or_default()
}

// ── spend grants: one-shot, bound, expiring ──────────────────────────────────

struct SpendGrant {
    token: String,
    kind: String,
    to: String,
    amount: String,
    /// Max total network fee (wei) the user accepted for THIS transfer, bound into
    /// the grant so a lying RPC can't inflate gasPrice*gasLimit past what was shown.
    /// 0 = unbounded (non-EVM callers / legacy path): backward-compatible.
    max_fee: u128,
    expires_ms: i64,
}

static SPENDS: OnceLock<Mutex<Vec<SpendGrant>>> = OnceLock::new();

fn spends() -> &'static Mutex<Vec<SpendGrant>> {
    SPENDS.get_or_init(|| Mutex::new(Vec::new()))
}

/// How long a confirmation stays valid. Long enough for the signer's network
/// round-trips to start, short enough that a stale confirm can't be replayed.
const SPEND_TTL_MS: i64 = 90_000;
/// At most this many un-redeemed grants may exist — a runaway minting loop is
/// a bug, and the cap turns it into a loud error instead of a queue.
const SPEND_CAP: usize = 8;

// ── optional hardware-bound spend authorization (fail-safe) ──────────────────
//
// The guard's honest gap (see header): an in-process caller can mint its own
// spend grant by calling the JNI directly, bypassing the UI BiometricPrompt.
// When a verification key is ENROLLED — the P-256 public key of an auth-required
// Android Keystore key — `authorize_spend` additionally requires a fresh
// signature over exactly (challenge, kind, to, amount), which only a real
// BiometricPrompt CryptoObject op can produce. Until a key is enrolled the
// behaviour is UNCHANGED (the UI biometric stays the gate); Kotlin enrolls only
// after a round-trip self-test, so a broken signing path can never lock the user
// out of spending.
static SPEND_VKEY: OnceLock<Vec<u8>> = OnceLock::new();
/// Per-process kill switch for the (OnceLock, process-global) spend binding so
/// the user can turn hardware confirmation OFF without a restart. enroll/reenroll
/// clears it; unenroll sets it. The Keystore key + the enrolled-pref persist, so
/// boot re-enrollment re-arms it when the user left it on.
static SPEND_DISABLED: AtomicBool = AtomicBool::new(false);
static SPEND_CHALLENGE: OnceLock<Mutex<Option<(String, i64)>>> = OnceLock::new();
fn spend_challenge_slot() -> &'static Mutex<Option<(String, i64)>> {
    SPEND_CHALLENGE.get_or_init(|| Mutex::new(None))
}
/// Disable (unenroll) gets its OWN challenge slot, mirroring the reveal slot, so a
/// concurrent spend and a "turn hardware confirmation off" cannot race-overwrite each
/// other's one-time challenge (M-3). Domain separation is preserved by the message
/// kind ("spend.unenroll"); this only fixes the slot collision.
static UNENROLL_CHALLENGE: OnceLock<Mutex<Option<(String, i64)>>> = OnceLock::new();
fn unenroll_challenge_slot() -> &'static Mutex<Option<(String, i64)>> {
    UNENROLL_CHALLENGE.get_or_init(|| Mutex::new(None))
}

/// True once a hardware spend-verification key is enrolled (binding enforced).
pub fn spend_binding_active() -> bool {
    SPEND_VKEY.get().is_some() && !SPEND_DISABLED.load(Ordering::Relaxed)
}

/// Enroll the SEC1 (uncompressed `0x04||X||Y`, 65-byte) P-256 public key of the
/// auth-required Keystore signing key. Rejected if it doesn't parse. Idempotent.
pub fn enroll_spend_key(sec1: &[u8]) -> Result<(), String> {
    use p256::ecdsa::VerifyingKey;
    VerifyingKey::from_sec1_bytes(sec1).map_err(|e| format!("bad spend key: {e}"))?;
    let _ = SPEND_VKEY.set(sec1.to_vec());
    SPEND_DISABLED.store(false, Ordering::Relaxed); // (re)arm; clears a prior unenroll
    audit("spend.enroll", json!({ "bytes": sec1.len() }));
    Ok(())
}

/// Turn the hardware spend binding OFF (per-process) without a restart — sends
/// revert to the UI-biometric + plain-mint path. The enrolled Keystore key is
/// untouched, so toggling back on (or a boot re-enroll) re-arms it.
///
/// H4: this must NOT be reachable by a bare in-process call when a binding is
/// ACTIVE — otherwise an attacker disables the binding then self-mints a grant.
/// When active, callers MUST use `unenroll_spend_key_hw` (signature-verified). This
/// bare path only succeeds when the binding is already inactive (idempotent reset)
/// or was never enrolled (the legacy unenroll that Kotlin uses on a no-binding device).
pub fn unenroll_spend_key() -> Result<(), String> {
    if spend_binding_active() {
        audit("spend.unenroll.deny", json!({ "reason": "binding active — fresh hardware proof required" }));
        return Err("turning off hardware confirmation needs your fingerprint/PIN".into());
    }
    SPEND_DISABLED.store(true, Ordering::Relaxed);
    audit("spend.unenroll", json!({}));
    Ok(())
}

/// Issue a one-time challenge the Keystore op must sign to DISABLE the binding (H4).
/// Uses a DEDICATED slot (M-3) so a disable and a concurrent spend can't race-overwrite
/// each other's challenge; the message kind ("spend.unenroll") domain-separates it from
/// a real spend.
pub fn issue_unenroll_challenge() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|e| format!("challenge entropy: {e}"))?;
    let ch: String = b.iter().map(|x| format!("{x:02x}")).collect();
    if let Ok(mut slot) = unenroll_challenge_slot().lock() {
        *slot = Some((ch.clone(), now_ms() + SPEND_TTL_MS));
    }
    Ok(ch)
}

/// Turn the binding OFF only after a fresh Keystore signature over the one-time
/// challenge verifies (H4): a mirror of `verify_spend_sig`, with the tuple
/// (challenge, "spend.unenroll", "spend.unenroll", "spend.unenroll"). An in-process
/// caller cannot forge this without a real biometric op, so it can no longer silently
/// disarm the binding.
pub fn unenroll_spend_key_hw(sig_hex: &str) -> Result<(), String> {
    let vk_bytes = SPEND_VKEY.get().ok_or("spend binding not enrolled")?;
    let challenge = {
        let mut slot = unenroll_challenge_slot().lock().map_err(|_| "challenge poisoned".to_string())?;
        match slot.take() {
            Some((c, exp)) if exp > now_ms() => c,
            _ => return Err("disable challenge missing or expired — try again".into()),
        }
    };
    verify_sig_inner(vk_bytes, &challenge, "spend.unenroll", "spend.unenroll", "spend.unenroll", sig_hex)
        .map_err(|e| {
            audit("spend.unenroll.deny", json!({ "reason": "disable signature rejected" }));
            e
        })?;
    SPEND_DISABLED.store(true, Ordering::Relaxed);
    audit("spend.unenroll", json!({ "verified": true }));
    Ok(())
}

/// Issue a one-time challenge the Keystore op must sign. Bounded by the spend TTL.
pub fn issue_spend_challenge() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|e| format!("challenge entropy: {e}"))?;
    let ch: String = b.iter().map(|x| format!("{x:02x}")).collect();
    if let Ok(mut slot) = spend_challenge_slot().lock() {
        *slot = Some((ch.clone(), now_ms() + SPEND_TTL_MS));
    }
    Ok(ch)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Pure verifier (no globals): the SHA256withECDSA/DER signature over
/// `challenge \0 kind \0 to \0 amount` against the SEC1 P-256 key. Unit-tested.
fn verify_sig_inner(
    vk_sec1: &[u8],
    challenge: &str,
    kind: &str,
    to: &str,
    amount: &str,
    sig_hex: &str,
) -> Result<(), String> {
    use p256::ecdsa::{signature::Verifier, DerSignature, VerifyingKey};
    let vk = VerifyingKey::from_sec1_bytes(vk_sec1).map_err(|e| format!("enrolled key: {e}"))?;
    let sig_bytes = hex_decode(sig_hex).ok_or("bad signature hex")?;
    let sig = DerSignature::try_from(sig_bytes.as_slice()).map_err(|e| format!("bad signature: {e}"))?;
    let msg = format!("{challenge}\0{kind}\0{to}\0{amount}");
    vk.verify(msg.as_bytes(), &sig)
        .map_err(|_| "spend signature did not verify".to_string())
}

/// Enrollment self-test: verify a signature over the fixed selftest tuple
/// against `sec1` WITHOUT enrolling. Lets Kotlin prove the Keystore-sign →
/// Rust-verify path works end-to-end on THIS device before flipping the binding
/// on, so a broken signer can never lock the user out of spending.
pub fn spend_selftest(sec1: &[u8], challenge: &str, sig_hex: &str) -> bool {
    verify_sig_inner(sec1, challenge, "selftest", "selftest", "selftest", sig_hex).is_ok()
}

/// Verify the Keystore signature over (challenge, kind, to, amount) against the
/// enrolled key, consuming the one-time challenge (single use, even on failure).
fn verify_spend_sig(kind: &str, to: &str, amount: &str, sig_hex: &str) -> Result<(), String> {
    let vk_bytes = SPEND_VKEY.get().ok_or("spend binding not enrolled")?;
    let challenge = {
        let mut slot = spend_challenge_slot().lock().map_err(|_| "challenge poisoned".to_string())?;
        match slot.take() {
            Some((c, exp)) if exp > now_ms() => c,
            _ => return Err("spend challenge missing or expired — confirm again".into()),
        }
    };
    verify_sig_inner(vk_bytes, &challenge, kind, to, amount, sig_hex)
}

// ── hardware-bound seed reveal (H5) ──────────────────────────────────────────
//
// The master mnemonic is the highest-privilege secret in the app — its reveal must
// be at least as protected as a single spend. When the spend binding is enrolled,
// revealing the phrase requires a fresh Keystore signature over a one-time
// reveal-challenge, verified HERE before the phrase ever leaves the runtime. The
// message tuple is domain-separated ("seed.reveal") so a spend signature can't be
// replayed as a reveal and vice-versa. Reuses the enrolled SPEND_VKEY + its
// per-op-auth Keystore key. Until a key is enrolled, the UI biometric stays the
// gate (the legacy `wallet_phrase` path).
static REVEAL_CHALLENGE: OnceLock<Mutex<Option<(String, i64)>>> = OnceLock::new();
fn reveal_challenge_slot() -> &'static Mutex<Option<(String, i64)>> {
    REVEAL_CHALLENGE.get_or_init(|| Mutex::new(None))
}

/// Issue a one-time challenge the Keystore op must sign to reveal the seed.
pub fn issue_reveal_challenge() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|e| format!("challenge entropy: {e}"))?;
    let ch: String = b.iter().map(|x| format!("{x:02x}")).collect();
    if let Ok(mut slot) = reveal_challenge_slot().lock() {
        *slot = Some((ch.clone(), now_ms() + SPEND_TTL_MS));
    }
    Ok(ch)
}

/// Verify the Keystore signature authorizing a seed reveal, consuming the one-time
/// challenge (single use, even on failure). Signed message is
/// `challenge \0 seed.reveal \0 seed.reveal \0 seed.reveal` — same shape as a spend
/// (so the same `signBound` path works) but domain-separated by the constant kind.
pub fn verify_reveal_sig(sig_hex: &str) -> Result<(), String> {
    let vk_bytes = SPEND_VKEY.get().ok_or("seed-reveal binding not enrolled")?;
    let challenge = {
        let mut slot = reveal_challenge_slot().lock().map_err(|_| "challenge poisoned".to_string())?;
        match slot.take() {
            Some((c, exp)) if exp > now_ms() => c,
            _ => return Err("reveal challenge missing or expired — try again".into()),
        }
    };
    verify_sig_inner(vk_bytes, &challenge, "seed.reveal", "seed.reveal", "seed.reveal", sig_hex).map_err(|e| {
        audit("seed.reveal.deny", json!({ "reason": "reveal signature rejected" }));
        e
    })
}

/// Mint a one-shot authorization for a specific transfer. Call ONLY after the
/// user has confirmed exactly this (kind, to, amount) on a trusted surface.
/// `sig_hex` carries the hardware proof when spend binding is enrolled (else None).
/// Convenience wrapper: no fee bound (max_fee = 0). EVM callers that want the
/// fee bound use `authorize_spend_fee`.
pub fn authorize_spend(kind: &str, to: &str, amount: &str, sig_hex: Option<&str>) -> Result<String, String> {
    authorize_spend_fee(kind, to, amount, 0, sig_hex)
}

/// Like `authorize_spend` but binds a maximum total network fee (wei) into the
/// grant. `redeem_spend_fee` then refuses to sign a tx whose actual fee exceeds it,
/// so a hostile RPC can't inflate gasPrice*gasLimit past what the user confirmed.
/// `max_fee = 0` = unbounded (backward-compatible with non-EVM callers).
pub fn authorize_spend_fee(
    kind: &str,
    to: &str,
    amount: &str,
    max_fee: u128,
    sig_hex: Option<&str>,
) -> Result<String, String> {
    if kind.is_empty() || to.is_empty() || amount.is_empty() {
        return Err("spend authorization requires kind, recipient and amount".into());
    }
    // Hardware binding (when enrolled): require a fresh Keystore signature over
    // exactly this (kind,to,amount). Fail-safe: no enrollment → skip, the UI
    // biometric remains the gate.
    if spend_binding_active() {
        match sig_hex {
            Some(sig) if !sig.is_empty() => verify_spend_sig(kind, to, amount, sig).map_err(|e| {
                audit("spend.deny", json!({ "reason": "spend signature rejected", "kind": kind }));
                e
            })?,
            _ => {
                audit("spend.deny", json!({ "reason": "hardware binding active, signature missing", "kind": kind }));
                return Err("this transfer must be confirmed with your fingerprint/PIN".into());
            }
        }
    }
    let mut g = spends().lock().map_err(|_| "spend grants poisoned".to_string())?;
    let now = now_ms();
    g.retain(|s| s.expires_ms > now);
    if g.len() >= SPEND_CAP {
        audit("spend.deny", json!({ "reason": "too many outstanding grants", "kind": kind }));
        return Err("too many outstanding spend authorizations".into());
    }
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| format!("spend token entropy: {e}"))?;
    let token: String = b.iter().map(|x| format!("{x:02x}")).collect();
    g.push(SpendGrant {
        token: token.clone(),
        kind: kind.to_string(),
        to: to.to_string(),
        amount: amount.to_string(),
        max_fee,
        expires_ms: now + SPEND_TTL_MS,
    });
    audit("spend.authorize", json!({ "kind": kind, "to": to, "amount": amount, "max_fee": max_fee.to_string() }));
    Ok(token)
}

/// Consume a grant. The transfer the signer is about to make must match the
/// grant EXACTLY — kind, recipient, amount — or the grant does not apply.
/// Single use: a matched grant is removed whether or not the send later fails.
/// No fee check (non-EVM callers): use `redeem_spend_fee` to enforce the max-fee.
pub fn redeem_spend(token: &str, kind: &str, to: &str, amount: &str) -> Result<(), String> {
    redeem_spend_fee(token, kind, to, amount, None)
}

/// Consume a grant AND verify the actual network fee. `actual_fee` (wei) is the
/// signer's computed gasPrice*gasLimit; when the grant carries a non-zero
/// `max_fee`, a fee above it is REJECTED (single-use: the grant is still consumed
/// so a retry must re-confirm). `actual_fee = None` skips the fee check (non-EVM).
pub fn redeem_spend_fee(
    token: &str,
    kind: &str,
    to: &str,
    amount: &str,
    actual_fee: Option<u128>,
) -> Result<(), String> {
    let mut g = spends().lock().map_err(|_| "spend grants poisoned".to_string())?;
    let now = now_ms();
    g.retain(|s| s.expires_ms > now);
    let idx = g.iter().position(|s| s.token == token);
    let Some(idx) = idx else {
        audit("spend.deny", json!({ "kind": kind, "to": to, "amount": amount, "reason": "no matching grant (missing, expired or reused)" }));
        return Err("transfer not authorized: confirm the transfer again (authorization missing, expired or already used)".into());
    };
    let s = g.remove(idx); // single use, even on mismatch below
    if s.kind != kind || s.to != to || s.amount != amount {
        audit("spend.deny", json!({ "kind": kind, "to": to, "amount": amount, "reason": "grant binding mismatch" }));
        return Err("transfer not authorized: it does not match what was confirmed".into());
    }
    // Fee bound (H6/max-fee): when the user confirmed a max fee, refuse a tx whose
    // real fee exceeds it — an inflated gasPrice from a lying RPC can't drain extra.
    if s.max_fee > 0 {
        if let Some(fee) = actual_fee {
            if fee > s.max_fee {
                audit("spend.deny", json!({ "kind": kind, "to": to, "amount": amount, "reason": "fee exceeds confirmed maximum", "max_fee": s.max_fee.to_string(), "actual_fee": fee.to_string() }));
                return Err(format!(
                    "transfer not authorized: the network fee ({fee} wei) is higher than the {} wei you confirmed — try again",
                    s.max_fee
                ));
            }
        }
    }
    audit("spend.redeem", json!({ "kind": kind, "to": to, "amount": amount }));
    Ok(())
}

// ── BEAM send cap (in-process, not the flippable Kotlin SharedPref) ──────────
//
// BEAM is Mimblewimble: the recipient/amount are NOT on-chain-public, so the spend
// grant still binds (kind="beam:<asset>", to=token, amount=decimal-BEAM) and is the
// real consent gate. The cap is defense-in-depth: until the user lifts it (after a
// successful test send), a BEAM transfer above SEND_CAP_GROTH is refused HERE, in
// Rust, regardless of any SharedPreferences boolean. The Kotlin cap is UX-only.

/// 0.01 BEAM (groth) — must match BeamApi.SEND_CAP_GROTH.
pub const BEAM_SEND_CAP_GROTH: u64 = 1_000_000;
/// Process-global "cap lifted" flag, set ONLY via `lift_beam_cap` (which Kotlin
/// calls behind a fresh hardware auth). Resets to false on every cold start, so a
/// stale SharedPref can't silently keep the cap lifted across launches.
static BEAM_CAP_LIFTED: AtomicBool = AtomicBool::new(false);

/// Lift the BEAM send cap for this process (call behind a fresh biometric/PIN).
pub fn lift_beam_cap() {
    BEAM_CAP_LIFTED.store(true, Ordering::Relaxed);
    audit("beam.cap.lift", json!({}));
}

/// Re-apply the BEAM send cap (the user toggled it off).
pub fn reset_beam_cap() {
    BEAM_CAP_LIFTED.store(false, Ordering::Relaxed);
    audit("beam.cap.reset", json!({}));
}

pub fn beam_cap_lifted() -> bool {
    BEAM_CAP_LIFTED.load(Ordering::Relaxed)
}

/// Enforce the BEAM cap in Rust. `amount_groth` above the cap is refused unless the
/// in-process cap-lifted flag is set. Loud + audited on denial.
pub fn check_beam_cap(amount_groth: u64) -> Result<(), String> {
    if amount_groth > BEAM_SEND_CAP_GROTH && !beam_cap_lifted() {
        audit("beam.cap.deny", json!({ "amount_groth": amount_groth, "cap_groth": BEAM_SEND_CAP_GROTH }));
        return Err(format!(
            "BEAM safety cap: first sends are limited to {} BEAM. Lift it in BEAM settings after a successful test send.",
            BEAM_SEND_CAP_GROTH as f64 / 100_000_000.0
        ));
    }
    Ok(())
}

/// Drop every outstanding grant — wired to lock/logout surfaces.
pub fn revoke_spends() {
    if let Ok(mut g) = spends().lock() {
        if !g.is_empty() {
            audit("spend.revoke_all", json!({ "count": g.len() }));
            g.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_table_covers_engine_ops() {
        for (scheme, op) in [
            ("identity", "sign"),
            ("peer", "gossip_send"),
            ("peer", "get_ticket"),
            ("blobs", "fetch"),
            ("content", "publish"),
            ("did", "resolve"),
        ] {
            assert!(check(scheme, op).is_ok(), "{scheme}/{op} must be granted");
        }
    }

    #[test]
    fn unknown_op_denied() {
        assert!(check("identity", "export_seed").is_err());
        assert!(check("peer", "shell").is_err());
    }

    #[test]
    fn spend_grant_is_one_shot_and_bound() {
        // No hardware key enrolled in tests → sig is None (fail-safe path).
        let t = authorize_spend("evm:esc", "0xabc", "0x1", None).unwrap();
        // Wrong binding consumes nothing? No — single use even on mismatch.
        assert!(redeem_spend(&t, "evm:esc", "0xOTHER", "0x1").is_err());
        // Already consumed.
        assert!(redeem_spend(&t, "evm:esc", "0xabc", "0x1").is_err());
        // Fresh grant redeems exactly once.
        let t2 = authorize_spend("ela", "EUq1...", "1.5", None).unwrap();
        assert!(redeem_spend(&t2, "ela", "EUq1...", "1.5").is_ok());
        assert!(redeem_spend(&t2, "ela", "EUq1...", "1.5").is_err());
    }

    #[test]
    fn max_fee_bound_rejects_inflated_fee() {
        // Grant with a max fee of 1000 wei.
        let t = authorize_spend_fee("evm:esc", "0xabc", "0x1", 1000, None).unwrap();
        // A fee above the bound is rejected (and consumes the grant — single use).
        assert!(redeem_spend_fee(&t, "evm:esc", "0xabc", "0x1", Some(1001)).is_err());
        assert!(redeem_spend_fee(&t, "evm:esc", "0xabc", "0x1", Some(500)).is_err()); // already consumed
        // A fresh grant with a fee at/under the bound redeems once.
        let t2 = authorize_spend_fee("evm:esc", "0xabc", "0x1", 1000, None).unwrap();
        assert!(redeem_spend_fee(&t2, "evm:esc", "0xabc", "0x1", Some(1000)).is_ok());
        // max_fee = 0 → unbounded (legacy / non-EVM): any fee, and a None fee, pass.
        let t3 = authorize_spend("ela", "EUq1", "1.5", None).unwrap();
        assert!(redeem_spend_fee(&t3, "ela", "EUq1", "1.5", Some(u128::MAX)).is_ok());
        let t4 = authorize_spend("ela", "EUq1", "1.5", None).unwrap();
        assert!(redeem_spend(&t4, "ela", "EUq1", "1.5").is_ok());
    }

    // Hardware spend-binding verifier (the pure path — no process globals, so it
    // doesn't pollute SPEND_VKEY for the other tests). Proves a real P-256
    // signature over (challenge,kind,to,amount) verifies and a tampered one fails.
    #[test]
    fn spend_signature_binding_verifies_and_rejects() {
        use p256::ecdsa::{signature::Signer, Signature, SigningKey};
        let sk = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let vk_sec1 = sk.verifying_key().to_encoded_point(false);
        let vk_sec1 = vk_sec1.as_bytes();
        let (ch, kind, to, amount) = ("deadbeef", "evm:esc", "0xabc", "0x1");
        let msg = format!("{ch}\0{kind}\0{to}\0{amount}");
        let sig: Signature = sk.sign(msg.as_bytes());
        let sig_hex: String = sig.to_der().as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        // Valid signature over the exact tuple verifies.
        assert!(verify_sig_inner(vk_sec1, ch, kind, to, amount, &sig_hex).is_ok());
        // A changed recipient (same signature) must fail — binds the destination.
        assert!(verify_sig_inner(vk_sec1, ch, kind, "0xEVIL", amount, &sig_hex).is_err());
        // A stale challenge must fail — one-time freshness.
        assert!(verify_sig_inner(vk_sec1, "00", kind, to, amount, &sig_hex).is_err());
        // Garbage signature is rejected, not panicked.
        assert!(verify_sig_inner(vk_sec1, ch, kind, to, amount, "zz").is_err());
    }
}
