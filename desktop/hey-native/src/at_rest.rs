//! At-rest sealing of the on-device identity, closing the desktop↔mobile parity
//! gap where desktop wrote `identity.json` (the BIP39 SEED) as PLAINTEXT while
//! mobile hardware-seals it.
//!
//! Mobile installs a hardware-wrapped 32-byte storage DEK via the runtime's
//! at-rest key BEFORE the runtime touches identity, so every storage write —
//! `identity.json` included — is ChaCha20-Poly1305 ciphertext on disk. Desktop
//! has no StrongBox/TEE, but every desktop OS ships a secret store (Linux
//! secret-service, macOS Keychain, Windows credential manager). We use the
//! `keyring` crate to get-or-create a 32-byte DEK there, then install it through
//! the EXACT same public plat fn the mobile bridge calls. The engine crate is
//! UNTOUCHED — we only USE its public `plat` surface.
//!
//! SAFETY (this whole module exists to never lose the seed):
//!   * The DEK install runs BEFORE `start_background`, so a NEW identity is born
//!     sealed and an EXISTING plaintext file migrates before the runtime reads it.
//!   * Migration is verify-before-replace: we seal to a TEMP file, round-trip
//!     OPEN it and byte-compare against the original, and ONLY THEN atomically
//!     rename over `identity.json`. Any failure deletes the temp and leaves the
//!     plaintext original untouched. We never delete a seed without a proven
//!     sealed copy already on disk.
//!   * If the keyring is unavailable (headless Linux without secret-service, a
//!     locked keychain, …) we log a warning and continue WITHOUT a DEK — the
//!     pre-existing plaintext behavior. The user always gets into the app.
//!   * Idempotent: every boot installs the same keyring DEK; once the file is
//!     sealed, migration is a no-op (we detect the at-rest magic and skip).

use std::path::Path;

use base64::Engine as _;

/// Keyring service ("application") name — the namespace the DEK lives under.
const KEYRING_SERVICE: &str = "hey-native";
/// Keyring account ("user") name — the entry within the service.
const KEYRING_ACCOUNT: &str = "at-rest-dek";

/// Install (and, if needed, mint) the storage DEK from the OS keyring, then
/// migrate an existing plaintext `identity.json` to sealed-at-rest. Best-effort:
/// any failure logs and returns, leaving the prior plaintext behavior intact.
///
/// MUST be called BEFORE `hey_mobile_runtime::start_background`, so the runtime's
/// identity read/write path sees the DEK active and the file already sealed.
pub fn install_and_migrate(data_dir: &Path) {
    let id_path = data_dir.join("identity.json");
    let dek = match get_or_create_dek() {
        Some(d) => d,
        None => {
            // No working keyring → no DEK. If identity.json is ALREADY SEALED (a
            // prior boot DID have a keyring, now lost/reset), proceeding would let
            // the runtime fail to decrypt it and MINT A FRESH identity OVER it =
            // SEED LOSS. Preserve the sealed seed by moving it aside so it is never
            // destroyed; the user restores from their recovery phrase, or — if the
            // keyring comes back — renames identity.json.locked back into place.
            if let Ok(bytes) = std::fs::read(&id_path) {
                if hey_core::crypto::is_at_rest(&bytes) {
                    let backup = data_dir.join("identity.json.locked");
                    if !backup.exists() {
                        let _ = std::fs::rename(&id_path, &backup);
                    }
                    log::error!(
                        "at-rest: identity.json is SEALED but the OS keyring is unavailable — \
                         preserved it as identity.json.locked (NOT overwritten). Restore from \
                         your recovery phrase, or fix the keyring and rename it back."
                    );
                }
            }
            // Otherwise (plaintext or no file): the runtime falls back to plaintext
            // exactly as the CLI does; the app still launches.
            log::warn!(
                "at-rest: OS keyring unavailable — identity stays PLAINTEXT \
                 (no hardware-equivalent sealing on this host)"
            );
            return;
        }
    };

    // Install through the same public plat fn the mobile JNI bridge uses. From
    // here on every storage write (identity.json included) seals transparently.
    hey_core::plat::set_at_rest_key(dek);
    log::info!("at-rest: storage DEK installed from OS keyring (identity sealed at rest)");

    // Proactively migrate a legacy plaintext identity.json. The runtime's own
    // load_or_create would also re-seal it on read, but we do a VERIFIED
    // seal-then-verify-then-rename here first so the migration can never lose the
    // seed even if it ran against an unexpected on-disk state.
    migrate_plaintext_identity(&id_path);
}

/// Get the DEK from the keyring, or mint + persist a fresh 32-byte one on first
/// run. `None` on ANY keyring/decoding error (caller falls back to plaintext).
fn get_or_create_dek() -> Option<[u8; 32]> {
    let entry = match keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("at-rest: cannot open keyring entry: {e}");
            return None;
        }
    };

    match entry.get_password() {
        Ok(b64) => {
            // Existing DEK — decode it. A malformed stored value is treated as a
            // keyring failure (fall back to plaintext); we do NOT overwrite it,
            // because a wrong key would make an already-sealed file unreadable.
            match decode_dek(&b64) {
                Some(k) => Some(k),
                None => {
                    log::warn!(
                        "at-rest: stored DEK is malformed — refusing to overwrite; \
                         continuing without sealing"
                    );
                    None
                }
            }
        }
        Err(keyring::Error::NoEntry) => {
            // First run on this host → mint a fresh DEK and store it.
            let mut key = [0u8; 32];
            if let Err(e) = getrandom::getrandom(&mut key) {
                log::warn!("at-rest: CSPRNG failed ({e}) — continuing without sealing");
                return None;
            }
            let b64 = base64::engine::general_purpose::STANDARD.encode(key);
            if let Err(e) = entry.set_password(&b64) {
                log::warn!("at-rest: could not store new DEK in keyring: {e} — continuing without sealing");
                return None;
            }
            log::info!("at-rest: minted + stored a new storage DEK in the OS keyring");
            Some(key)
        }
        Err(e) => {
            log::warn!("at-rest: keyring read failed: {e} — continuing without sealing");
            None
        }
    }
}

/// Decode a base64 DEK string back to exactly 32 bytes. `None` if it does not
/// decode or is the wrong length.
fn decode_dek(b64: &str) -> Option<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

/// VERIFY-BEFORE-REPLACE migration of a plaintext `identity.json`.
///
/// Ordering (every branch leaves the seed recoverable):
///   1. If the file is missing → nothing to migrate (a fresh identity will be
///      born sealed by the runtime).
///   2. If it is already at-rest (carries the magic) → no-op (idempotent).
///   3. Seal the raw bytes → write to `identity.json.tmp`.
///   4. Read the tmp BACK, OPEN it with the DEK, and require the recovered bytes
///      to equal the original EXACTLY.
///   5. ONLY on a passing round-trip, atomically rename tmp → identity.json.
///   6. On ANY failure at steps 3-5, delete the tmp and RETURN, leaving the
///      original plaintext file untouched. We never replace/delete the seed
///      without a verified sealed copy in hand.
fn migrate_plaintext_identity(id_path: &Path) {
    let original = match std::fs::read(id_path) {
        Ok(b) => b,
        // Missing file → nothing to do. The runtime mints a sealed one.
        Err(_) => return,
    };

    // Already sealed (magic present) → idempotent no-op.
    if hey_core::crypto::is_at_rest(&original) {
        log::debug!("at-rest: identity.json already sealed — migration skipped");
        return;
    }

    // Sanity: only treat it as a migratable plaintext identity if it parses as a
    // JSON object. Anything else we leave strictly alone (never touch unknown
    // bytes that hold a seed).
    if serde_json::from_slice::<serde_json::Value>(&original)
        .ok()
        .filter(|v| v.is_object())
        .is_none()
    {
        log::warn!(
            "at-rest: identity.json is neither sealed nor a JSON object — leaving it \
             UNTOUCHED (will not risk the seed)"
        );
        return;
    }

    // Step 3: seal to a temp file.
    let enc = match hey_core::plat::seal_with_at_rest_key(&original) {
        Some(e) => e,
        None => {
            // Should not happen (we just installed the DEK), but never proceed
            // without ciphertext.
            log::warn!("at-rest: seal returned None (no DEK?) — leaving identity.json plaintext");
            return;
        }
    };
    let tmp_path = id_path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &enc) {
        log::warn!("at-rest: could not write sealed temp file: {e} — leaving identity.json plaintext");
        let _ = std::fs::remove_file(&tmp_path);
        return;
    }

    // Step 4: read the temp BACK and verify the round-trip against the original.
    let verified = verify_sealed_temp(&tmp_path, &original);
    if !verified {
        log::warn!(
            "at-rest: sealed-copy verification FAILED — discarding temp, identity.json \
             stays PLAINTEXT (seed preserved)"
        );
        let _ = std::fs::remove_file(&tmp_path);
        return;
    }

    // Step 5: verified — atomically replace the plaintext file.
    match std::fs::rename(&tmp_path, id_path) {
        Ok(()) => log::info!("at-rest: migrated identity.json to sealed-at-rest (verified round-trip)"),
        Err(e) => {
            // Rename failed → original plaintext is still in place. Clean up.
            log::warn!("at-rest: atomic rename failed: {e} — identity.json stays plaintext (seed preserved)");
            let _ = std::fs::remove_file(&tmp_path);
        }
    }
}

/// Read the sealed temp file back from disk, OPEN it with the installed DEK, and
/// confirm the recovered plaintext equals `original` byte-for-byte. Returns true
/// only on a full, on-disk round-trip — never trusts the in-memory blob alone.
fn verify_sealed_temp(tmp_path: &Path, original: &[u8]) -> bool {
    let back = match std::fs::read(tmp_path) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("at-rest: could not re-read sealed temp for verification: {e}");
            return false;
        }
    };
    // It must look sealed AND open to the exact original bytes.
    if !hey_core::crypto::is_at_rest(&back) {
        log::warn!("at-rest: sealed temp lacks the at-rest magic");
        return false;
    }
    match hey_core::plat::open_with_at_rest_key(&back) {
        Some(plain) if plain == original => true,
        Some(_) => {
            log::warn!("at-rest: round-trip mismatch (decrypted bytes differ from original)");
            false
        }
        None => {
            log::warn!("at-rest: round-trip open failed (tag/key mismatch)");
            false
        }
    }
}
