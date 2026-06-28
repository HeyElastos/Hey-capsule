//! Ledger Elastos app (`LedgerHQ/app-elastos`) — GET_PUBLIC_KEY + SIGN over the
//! BLE transport in [`crate::ledger_ble`]. The app signs CX_SHA256 of the bytes we
//! send with deterministic RFC-6979 P-256 (secp256r1) — the SAME curve + hash as
//! Hey's local `ela_sign`, which is what makes a Ledger-derived address
//! Essentials-interoperable (docs/HEY_LEDGER_SUPPORT.md §3.3).
//!
//! Protocol (verified from app-elastos + Essentials' hw-app-ela):
//!   CLA 0x80; INS_GET_PUBLIC_KEY 0x04, INS_SIGN 0x02.
//!   GET_PUBLIC_KEY: `80 04 00 00 00 <20-byte path>` → 65-byte uncompressed 04‖X‖Y.
//!   SIGN: chunk `unsigned_tx ‖ 20-byte path` ≤255B; P1 0x00 (more) / 0x80 (last).
//!         The device strips the trailing 20 path bytes, hashes TX-only, derives the
//!         key from the path, signs. Response = DER TLV ‖ FFFF ‖ 32-byte sha256.

use std::time::Duration;

use crate::ledger_ble;

const CLA: u8 = 0x80;
const INS_GET_PUBLIC_KEY: u8 = 0x04;
const INS_SIGN: u8 = 0x02;
const P1_MORE: u8 = 0x00;
const P1_LAST: u8 = 0x80;
/// The app's unsigned-tx ceiling ("1024 does not work correctly"); the 20-byte path
/// suffix rides on top of the tx.
const MAX_UNSIGNED_TX: usize = 1000;
/// A SIGN/GET op waits on a human pressing the device button — be generous.
const APDU_TIMEOUT: Duration = Duration::from_secs(60);
const NEG_TIMEOUT: Duration = Duration::from_secs(3);

/// "m/44'/2305'/0'/0/0" → 20-byte BIP44 path (5 × u32 BE; hardened sets the high bit).
fn encode_path(path: &str) -> Result<[u8; 20], String> {
    let p = path.trim().trim_start_matches("m/").trim_start_matches('/');
    let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() != 5 {
        return Err(format!("ELA path must have 5 levels, got {}", segs.len()));
    }
    let mut out = [0u8; 20];
    for (i, s) in segs.iter().enumerate() {
        let hardened = s.ends_with('\'') || s.ends_with('h') || s.ends_with('H');
        let num: u32 = s
            .trim_end_matches(['\'', 'h', 'H'])
            .parse()
            .map_err(|_| format!("bad path segment {s:?}"))?;
        let v = if hardened { num | 0x8000_0000 } else { num };
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    Ok(out)
}

/// GET_PUBLIC_KEY at `path` → 33-byte compressed P-256 pubkey.
pub fn get_pubkey(path: &str) -> Result<[u8; 33], String> {
    let pb = encode_path(path)?;
    ledger_ble::ensure_negotiated(NEG_TIMEOUT);
    // Canonical demo/Essentials send Lc=0x00 with the path as the data; the device
    // validates by total received length, not Lc (docs §3.3).
    let mut apdu = vec![CLA, INS_GET_PUBLIC_KEY, 0x00, 0x00, 0x00];
    apdu.extend_from_slice(&pb);
    let resp = ledger_ble::exchange(&apdu, APDU_TIMEOUT)?;
    parse_pubkey(&resp)
}

/// Device returns a 65-byte uncompressed key (sometimes 1-byte length-prefixed). The
/// host compresses to 33 bytes — the device does NOT return the compressed form.
fn parse_pubkey(resp: &[u8]) -> Result<[u8; 33], String> {
    let pk65: &[u8] = if resp.len() >= 65 && resp[0] == 0x04 {
        &resp[..65]
    } else if resp.len() >= 66 && resp[0] == 0x41 && resp[1] == 0x04 {
        &resp[1..66]
    } else {
        return Err(format!("ledger: unexpected GET_PUBLIC_KEY response ({} bytes)", resp.len()));
    };
    compress(pk65)
}

fn compress(pk65: &[u8]) -> Result<[u8; 33], String> {
    if pk65.len() != 65 || pk65[0] != 0x04 {
        return Err("ledger: not an uncompressed point".into());
    }
    let mut out = [0u8; 33];
    out[0] = if pk65[64] & 1 == 1 { 0x03 } else { 0x02 }; // parity of Y
    out[1..].copy_from_slice(&pk65[1..33]); // X
    Ok(out)
}

/// SIGN `unsigned_tx` at `path` → 64-byte low-S r‖s. We append the path as a SUFFIX
/// (the device strips the trailing 20 bytes before hashing, so its digest equals
/// sha256(unsigned_tx) — identical to Hey's local signer).
pub fn sign(path: &str, unsigned_tx: &[u8]) -> Result<[u8; 64], String> {
    if unsigned_tx.len() > MAX_UNSIGNED_TX {
        return Err(format!(
            "tx is {} bytes — over the Ledger ~{MAX_UNSIGNED_TX}-byte cap (consolidate UTXOs)",
            unsigned_tx.len()
        ));
    }
    let pb = encode_path(path)?;
    ledger_ble::ensure_negotiated(NEG_TIMEOUT);
    let mut payload = unsigned_tx.to_vec();
    payload.extend_from_slice(&pb); // path suffix

    let chunks: Vec<&[u8]> = payload.chunks(255).collect();
    let mut last = Vec::new();
    for (i, ch) in chunks.iter().enumerate() {
        let p1 = if i + 1 == chunks.len() { P1_LAST } else { P1_MORE };
        let mut apdu = vec![CLA, INS_SIGN, p1, 0x00, ch.len() as u8];
        apdu.extend_from_slice(ch);
        last = ledger_ble::exchange(&apdu, APDU_TIMEOUT)?;
    }
    parse_signature(&last)
}

/// Parse the SIGN response. Layout: `[DER: 30 LL 02 LR R 02 LS S]` then a non-standard
/// `FF FF` marker then a 32-byte sha256 (device debug artifact). We read the LEADING
/// DER TLV (R/S can be 0x20 or 0x21 bytes — read the lengths) and enforce low-S.
fn parse_signature(resp: &[u8]) -> Result<[u8; 64], String> {
    if resp.len() < 8 || resp[0] != 0x30 {
        return Err("ledger sig: missing DER header".into());
    }
    let mut i = 2; // skip 0x30 + total-length byte
    if resp.get(i) != Some(&0x02) {
        return Err("ledger sig: expected R INTEGER".into());
    }
    let lr = *resp.get(i + 1).ok_or("ledger sig: truncated R length")? as usize;
    i += 2;
    let r = resp.get(i..i + lr).ok_or("ledger sig: truncated R")?;
    i += lr;
    if resp.get(i) != Some(&0x02) {
        return Err("ledger sig: expected S INTEGER".into());
    }
    let ls = *resp.get(i + 1).ok_or("ledger sig: truncated S length")? as usize;
    i += 2;
    let s = resp.get(i..i + ls).ok_or("ledger sig: truncated S")?;
    normalize_low_s(der_int_to_32(r)?, der_int_to_32(s)?)
}

/// A DER INTEGER → fixed 32 bytes: drop a single 0x00 sign pad, then left-pad.
fn der_int_to_32(b: &[u8]) -> Result<[u8; 32], String> {
    let b = if b.len() == 33 && b[0] == 0x00 { &b[1..] } else { b };
    if b.is_empty() || b.len() > 32 {
        return Err("ledger sig: integer out of range".into());
    }
    let mut out = [0u8; 32];
    out[32 - b.len()..].copy_from_slice(b);
    Ok(out)
}

/// Build a canonical low-S signature (the SDK + consensus require it; neither the app
/// nor Essentials enforce it, so we do — same as the local signer).
fn normalize_low_s(r: [u8; 32], s: [u8; 32]) -> Result<[u8; 64], String> {
    use p256::ecdsa::Signature;
    let r_fb = p256::FieldBytes::clone_from_slice(&r);
    let s_fb = p256::FieldBytes::clone_from_slice(&s);
    let sig = Signature::from_scalars(r_fb, s_fb).map_err(|e| format!("ledger sig: {e}"))?;
    let sig = sig.normalize_s().unwrap_or(sig);
    let mut out = [0u8; 64];
    out.copy_from_slice(&sig.to_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_encodes_2305() {
        // m/44'/2305'/0'/0/0 → 8000002C 80000901 80000000 00000000 00000000
        let p = encode_path("m/44'/2305'/0'/0/0").unwrap();
        assert_eq!(
            p,
            [
                0x80, 0x00, 0x00, 0x2C, 0x80, 0x00, 0x09, 0x01, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
        // 'h' hardened marker is accepted too.
        assert_eq!(encode_path("44h/2305h/0h/0/0").unwrap(), p);
    }

    #[test]
    fn path_rejects_wrong_depth() {
        assert!(encode_path("m/44'/2305'/0'").is_err());
        assert!(encode_path("m/44'/2305'/0'/0/0/0").is_err());
    }

    #[test]
    fn compress_parity() {
        // Y even → 0x02, Y odd → 0x03.
        let mut pk = [0u8; 65];
        pk[0] = 0x04;
        for i in 1..33 {
            pk[i] = i as u8;
        }
        pk[64] = 0x02; // even
        assert_eq!(compress(&pk).unwrap()[0], 0x02);
        pk[64] = 0x07; // odd
        let c = compress(&pk).unwrap();
        assert_eq!(c[0], 0x03);
        assert_eq!(&c[1..], &pk[1..33]);
    }

    #[test]
    fn parse_pubkey_handles_length_prefix() {
        let mut raw = vec![0x04];
        raw.extend((1u8..=64).collect::<Vec<u8>>());
        assert!(parse_pubkey(&raw).is_ok());
        let mut prefixed = vec![0x41];
        prefixed.extend_from_slice(&raw);
        assert_eq!(parse_pubkey(&prefixed).unwrap(), parse_pubkey(&raw).unwrap());
    }

    // Build a DER ECDSA sig (30 LL 02 LR R 02 LS S) for a given r/s, optionally with
    // a 0x00 sign-pad, then the FFFF + 32-byte-hash trailer the device appends.
    fn der(r: &[u8], s: &[u8]) -> Vec<u8> {
        let mut body = vec![0x02, r.len() as u8];
        body.extend_from_slice(r);
        body.push(0x02);
        body.push(s.len() as u8);
        body.extend_from_slice(s);
        let mut out = vec![0x30, body.len() as u8];
        out.extend_from_slice(&body);
        out.extend_from_slice(&[0xFF, 0xFF]);
        out.extend_from_slice(&[0u8; 32]);
        out
    }

    #[test]
    fn parse_signature_20_and_21_byte_ints() {
        // 32-byte R/S (length 0x20).
        let r = [0x11u8; 32];
        let s = [0x22u8; 32];
        let parsed = parse_signature(&der(&r, &s)).unwrap();
        assert_eq!(&parsed[..32], &r);
        // S is low here, so it's unchanged.
        assert_eq!(&parsed[32..], &s);

        // 33-byte sign-padded R (high bit set → DER 0x00 pad), length 0x21.
        let mut rp = vec![0x00u8];
        rp.extend_from_slice(&[0x80u8; 32]); // high bit set
        let parsed2 = parse_signature(&der(&rp, &s)).unwrap();
        assert_eq!(&parsed2[..32], &[0x80u8; 32]);
    }

    #[test]
    fn parse_signature_enforces_low_s() {
        // S just above n/2 must be flipped to n - S (low-S). n/2 for P-256:
        // 0x7FFFFFFF800000007FFFFFFFFFFFFFFFDE737D56D38BCF4279DCE5617E3192A8
        // Use S = n/2 + 1's-ish high value; assert the parser returns a DIFFERENT
        // (normalized) S than the raw input for a known high-S.
        let r = [0x05u8; 32];
        // high-S: 0xFF.. is > n/2 → must normalize.
        let high_s = [0xFFu8; 32];
        // 0xFF..FF as a scalar may be >= n; from_scalars would reject. Use a value
        // that's a valid scalar but > n/2: n-1.
        let n_minus_1: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xBC, 0xE6, 0xFA, 0xAD, 0xA7, 0x17, 0x9E, 0x84, 0xF3, 0xB9, 0xCA, 0xC2,
            0xFC, 0x63, 0x25, 0x50,
        ];
        let _ = high_s;
        let parsed = parse_signature(&der(&r, &n_minus_1)).unwrap();
        // n-1 is high-S → normalized to 1 (n - (n-1) = 1).
        let mut one = [0u8; 32];
        one[31] = 1;
        assert_eq!(&parsed[32..], &one, "high-S must be flipped to low-S");
    }
}
