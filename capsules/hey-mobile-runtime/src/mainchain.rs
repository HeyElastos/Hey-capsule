//! Elastos MAINCHAIN (Elastos.ELA) standard single-sig transfer — build + sign +
//! broadcast a TransferAsset tx, BYTE-EXACT to the official wallet SDK (the one
//! Essentials uses), cross-checked against the Go consensus node.
//!
//! Byte-order landmines (verified):
//!   * txid (input) AND assetID (output) are written BYTE-REVERSED vs their display
//!     hex (the SDK parses display-hex as a bignum and emits it little-endian).
//!   * programHash (21B) is written RAW (already wire order).
//!   * value = int64 LE sela; index = u16 LE; sequence/outputLock/lockTime = u32 LE.
//!   * tx version 9: prefix `09 02 00` (version flag, TxType TransferAsset, payloadVer),
//!     each output ends with OutputType `0x00` (Default) + empty payload.
//!   * sign = SINGLE sha256 of the unsigned tx (no programs) → P-256 (secp256r1)
//!     RFC-6979 deterministic, LOW-S, 64-byte r‖s. program parameter =
//!     varbytes(0x40‖sig) (on the wire `41 40 <sig>` — the 0x40 push opcode is
//!     REQUIRED or the node rejects "invalid signature length"); code =
//!     varbytes(0x21‖pubkey‖0xAC). The P-256 signer is proven against the SDK's
//!     own golden digest→signature vector (see tests).
//! The signing key derives from the SAME mnemonic as everything else (did.rs P-256,
//! m/44'/0'/0'/0/0) — Essentials-recoverable.

use p256::ecdsa::SigningKey;
use serde_json::{json, Value};

use crate::did;

const RPC: &str = "https://api.elastos.io/ela";

/// The bundled public ELA mainchain RPC default — surfaced to the wallet UI so a
/// self-host override field can show it as the placeholder.
pub(crate) fn default_rpc() -> &'static str {
    RPC
}
/// ELA native asset id (DISPLAY form). On the wire it is byte-REVERSED.
const ELA_ASSET_DISPLAY: &str = "a3d0eaa466df74983b5d7c543de6904f4c9418ead5ffd6d25814234a96db37b0";
/// The SDK's standard simple-transfer fee: 10000 sela = 0.0001 ELA.
const FEE_SELA: i64 = 10_000;
const SELA_PER_ELA: i64 = 100_000_000;

// ── P-256 signer (RFC-6979 deterministic, low-S) — proven vs the SDK golden vector ──

fn ela_sign(priv_bytes: &[u8; 32], digest: &[u8; 32]) -> Result<[u8; 64], String> {
    let sk = SigningKey::from_bytes(priv_bytes.into()).map_err(|e| format!("ela key: {e}"))?;
    let (sig, _recid) = sk.sign_prehash_recoverable(digest).map_err(|e| format!("ela sign: {e}"))?;
    // The SDK uses canonical LOW-S; p256's recoverable signer does NOT normalize, so
    // enforce it here (RFC-6979 nonce already matches → R is identical).
    let sig = sig.normalize_s().unwrap_or(sig);
    let rs = sig.to_bytes(); // 64 bytes r||s, low-S
    let mut out = [0u8; 64];
    out.copy_from_slice(&rs);
    Ok(out)
}

// ── signer seam: where the ELA signature comes from ─────────────────────────
//
// The two facts that make this a clean seam (see docs/HEY_LEDGER_SUPPORT.md §1):
//   1. the private key is used in exactly ONE place — the sign — so Ledger can
//      replace it and the key need never exist locally;
//   2. the pubkey (needed for the change-output programHash AND the program
//      script 0x21‖pubkey‖0xAC) is fetched once at add-time and is NOT secret.
// `Local` is byte-for-byte today's behavior; the golden vector test still pins it.

/// Source of the ELA signature for a transfer.
pub enum ElaSigner<'a> {
    /// Derive the P-256 key from the seed and sign locally (today's path).
    Local { mnemonic: &'a str },
    /// Ask a connected Ledger to sign over BLE. `pubkey` is the 33-byte compressed
    /// P-256 key the device returned at add-time (so a send needs no extra
    /// GET_PUBLIC_KEY round-trip); `path` is the BIP44 path the key lives at.
    Ledger { pubkey: [u8; 33], path: String },
}

impl ElaSigner<'_> {
    /// The compressed P-256 pubkey for this signer — drives the change-output
    /// address and the program script. Local derives it; Ledger returns the stored one.
    fn pubkey(&self) -> Result<[u8; 33], String> {
        match self {
            ElaSigner::Local { mnemonic } => did::derive_p256(mnemonic, 0).map(|(_, pk)| pk),
            ElaSigner::Ledger { pubkey, .. } => Ok(*pubkey),
        }
    }

    /// 64-byte r‖s for this unsigned tx.
    ///   Local : sign sha256(unsigned) with the seed-derived key (UNCHANGED behavior).
    ///   Ledger: hand the device the unsigned BYTES; the app computes CX_SHA256 itself
    ///           and strips the trailing BIP44 path suffix before hashing.
    fn sign_unsigned(&self, unsigned: &[u8]) -> Result<[u8; 64], String> {
        match self {
            ElaSigner::Local { mnemonic } => {
                let (priv_bytes, _pk) = did::derive_p256(mnemonic, 0)?;
                // Wipe the signing scalar from the heap once this transfer finishes.
                let priv_bytes = zeroize::Zeroizing::new(priv_bytes);
                ela_sign(&priv_bytes, &sha256(unsigned))
            }
            ElaSigner::Ledger { path, .. } => crate::ledger_ela::sign(path, unsigned),
        }
    }

    /// Audit tag for the money log.
    fn via(&self) -> &'static str {
        match self {
            ElaSigner::Local { .. } => "seed",
            ElaSigner::Ledger { .. } => "ledger",
        }
    }
}

// ── byte helpers ───────────────────────────────────────────────────────────

fn hex_lower(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hex_decode(h: &str) -> Result<Vec<u8>, String> {
    let h = h.trim().trim_start_matches("0x");
    if h.len() % 2 != 0 {
        return Err("odd hex".into());
    }
    let b = h.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    let val = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err("bad hex".into()),
        }
    };
    let mut i = 0;
    while i < b.len() {
        out.push((val(b[i])? << 4) | val(b[i + 1])?);
        i += 2;
    }
    Ok(out)
}

/// 32-byte display hex → wire bytes (reversed).
fn rev32(display_hex: &str) -> Result<[u8; 32], String> {
    let mut b = hex_decode(display_hex)?;
    if b.len() != 32 {
        return Err("expected 32-byte hash".into());
    }
    b.reverse();
    Ok(b.try_into().unwrap())
}

/// Bitcoin/Elastos VarUint.
fn write_varuint(out: &mut Vec<u8>, n: u64) {
    if n < 0xfd {
        out.push(n as u8);
    } else if n <= 0xffff {
        out.push(0xfd);
        out.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n <= 0xffff_ffff {
        out.push(0xfe);
        out.extend_from_slice(&(n as u32).to_le_bytes());
    } else {
        out.push(0xff);
        out.extend_from_slice(&n.to_le_bytes());
    }
}

fn write_varbytes(out: &mut Vec<u8>, b: &[u8]) {
    write_varuint(out, b.len() as u64);
    out.extend_from_slice(b);
}

fn sha256(b: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().into()
}

/// Decimal ELA string ("1.50714932") → i64 sela, exact (no float).
fn ela_str_to_sela(s: &str) -> Result<i64, String> {
    let s = s.trim();
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    // Strict digits only: a stray sign ("1.-5") must not silently under-count.
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err("bad ELA amount".into());
    }
    let mut frac = frac.to_string();
    if frac.len() > 8 {
        frac.truncate(8);
    }
    while frac.len() < 8 {
        frac.push('0');
    }
    let w: i64 = whole.parse().map_err(|_| "bad ELA amount".to_string())?;
    let f: i64 = if frac.is_empty() { 0 } else { frac.parse().map_err(|_| "bad ELA frac".to_string())? };
    // Explicit overflow rejection: saturating arithmetic would silently cap a
    // huge amount to i64::MAX sela and send far less than the user confirmed.
    w.checked_mul(SELA_PER_ELA)
        .and_then(|sela| sela.checked_add(f))
        .ok_or_else(|| "ELA amount too large (max ~9.2 billion)".to_string())
}

fn format_ela(sela: i64) -> String {
    let whole = sela / SELA_PER_ELA;
    let frac = (sela % SELA_PER_ELA).abs();
    if frac == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac:08}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ── tx serialization (byte-exact) ───────────────────────────────────────────

struct Utxo {
    txid_display: String,
    index: u16,
    value_sela: i64,
}

/// Serialize the UNSIGNED tx (everything except the programs section) — this is
/// what gets sha256'd for signing. `outputs` = (programHash21, value_sela).
fn serialize_unsigned(inputs: &[Utxo], outputs: &[([u8; 21], i64)], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let mut o = Vec::new();
    o.push(0x09); // tx version flag (>= V09)
    o.push(0x02); // TxType = TransferAsset
    o.push(0x00); // payloadVersion (TransferAsset payload is empty)
    // attributes: a single Nonce attribute (usage 0x00) for tx uniqueness
    write_varuint(&mut o, 1);
    o.push(0x00);
    write_varbytes(&mut o, nonce);
    // inputs
    write_varuint(&mut o, inputs.len() as u64);
    for u in inputs {
        let mut txid = hex_decode(&u.txid_display)?;
        if txid.len() != 32 {
            return Err("bad utxo txid".into());
        }
        txid.reverse(); // display → wire
        o.extend_from_slice(&txid);
        o.extend_from_slice(&u.index.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // sequence
    }
    // outputs
    let asset = rev32(ELA_ASSET_DISPLAY)?;
    write_varuint(&mut o, outputs.len() as u64);
    for (ph, val) in outputs {
        o.extend_from_slice(&asset); // 32B assetID (reversed)
        o.extend_from_slice(&val.to_le_bytes()); // int64 LE sela
        o.extend_from_slice(&0u32.to_le_bytes()); // outputLock
        o.extend_from_slice(ph); // 21B programHash (raw)
        o.push(0x00); // v9: OutputType Default
        // OutputPayload Default: empty
    }
    o.extend_from_slice(&0u32.to_le_bytes()); // lockTime
    Ok(o)
}

fn serialize_signed(unsigned: &[u8], sig: &[u8; 64], pubkey: &[u8; 33]) -> Vec<u8> {
    let mut o = unsigned.to_vec();
    write_varuint(&mut o, 1); // 1 program
    // parameter CONTENT = 0x40 (push-64 opcode) ‖ 64-byte signature, then written as varbytes
    // → on the wire `41 40 <sig>`. ELA's checkStandardSignature requires the 65-byte parameter
    // (push opcode + sig); emitting just `varbytes(sig)` (`40 <sig>`) yields a 64-byte parameter
    // and the node rejects with "invalid signature length" (code 43001).
    let mut param = Vec::with_capacity(65);
    param.push(0x40);
    param.extend_from_slice(sig);
    write_varbytes(&mut o, &param);
    let mut code = Vec::with_capacity(35);
    code.push(0x21);
    code.extend_from_slice(pubkey);
    code.push(0xAC);
    write_varbytes(&mut o, &code); // code = 0x23 ‖ (0x21 ‖ pubkey ‖ 0xAC)
    o
}

// ── JSON-RPC (api.elastos.io/ela over TLS) ──────────────────────────────────

fn rpc(method: &str, params: Value) -> Result<Value, String> {
    let r = rpc_call(method, params);
    if let Err(e) = &r {
        log::warn!("[ela] {e}");
    }
    r
}

fn rpc_call(method: &str, params: Value) -> Result<Value, String> {
    let body = json!({ "method": method, "params": params });
    // Self-host override: <data_dir>/ela-rpc.txt (else the bundled default RPC).
    let resp = ureq::post(&crate::wallet::rpc_override("ela", RPC))
        .timeout(std::time::Duration::from_secs(20))
        .send_json(body)
        .map_err(|e| format!("ela rpc {method}: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("ela rpc {method} decode: {e}"))?;
    if let Some(err) = v.get("error") {
        if !err.is_null() {
            return Err(format!("ela rpc {method}: {err}"));
        }
    }
    v.get("result").cloned().ok_or_else(|| format!("ela rpc {method}: no result"))
}

fn list_unspent(addr: &str) -> Result<Vec<Utxo>, String> {
    let r = rpc("listunspent", json!({ "addresses": [addr], "utxotype": "normal" }))?;
    let arr = match r.as_array() {
        Some(a) => a,
        None => return Ok(Vec::new()), // no UTXOs → "no result"/empty
    };
    let mut out = Vec::new();
    for u in arr {
        if u.get("assetid").and_then(Value::as_str) != Some(ELA_ASSET_DISPLAY) {
            continue;
        }
        if u.get("outputlock").and_then(Value::as_i64).unwrap_or(0) != 0 {
            continue; // locked
        }
        let txid = u.get("txid").and_then(Value::as_str).unwrap_or("").to_string();
        let vout = u.get("vout").and_then(Value::as_u64).unwrap_or(0);
        if vout > u16::MAX as u64 {
            continue; // can't encode as the wire u16 index — skip rather than truncate
        }
        let index = vout as u16;
        let amount = u.get("amount").and_then(Value::as_str).unwrap_or("0");
        if txid.len() == 64 {
            // Skip-and-log a malformed UTXO amount rather than failing the WHOLE
            // balance/send — a hostile/MitM RPC could otherwise disable ELA with one
            // bad entry (audit RPC-PANIC-001).
            match ela_str_to_sela(amount) {
                Ok(value_sela) => out.push(Utxo { txid_display: txid, index, value_sela }),
                Err(e) => log::warn!("skipping malformed UTXO (amount={amount:?}): {e}"),
            }
        }
    }
    Ok(out)
}

// ── public API ──────────────────────────────────────────────────────────────

/// `{ address, sela, ela }` — spendable mainchain balance (sum of normal UTXOs).
pub fn ela_balance(mnemonic: &str) -> Result<Value, String> {
    let addr = did::ela_mainchain_address(mnemonic)?;
    let total: i64 = list_unspent(&addr)?.iter().map(|u| u.value_sela).sum();
    Ok(json!({ "address": addr, "sela": total.to_string(), "ela": format_ela(total) }))
}

/// MONEY: build + sign + broadcast a standard ELA mainchain transfer from the seed
/// wallet. `amount_ela` is a decimal string. Returns the tx hash.
pub fn ela_send(mnemonic: &str, to: &str, amount_ela: &str) -> Result<Value, String> {
    ela_send_with(ElaSigner::Local { mnemonic }, to, amount_ela)
}

/// MONEY: the same transfer, signed by a connected Ledger. `pubkey_hex` is the
/// 33-byte compressed P-256 key the device returned at add-time (so we don't pay a
/// second GET_PUBLIC_KEY round-trip), `path` its BIP44 path. The device shows
/// amount+recipient and the user presses to sign.
pub fn ela_send_ledger(path: &str, pubkey_hex: &str, to: &str, amount_ela: &str) -> Result<Value, String> {
    let pk = hex_decode(pubkey_hex)?;
    let pubkey: [u8; 33] =
        pk.as_slice().try_into().map_err(|_| "ledger pubkey must be 33 bytes".to_string())?;
    ela_send_with(ElaSigner::Ledger { pubkey, path: path.to_string() }, to, amount_ela)
}

/// Build + sign + broadcast a standard ELA mainchain transfer, routing the signature
/// through `signer`. The seed path (`ElaSigner::Local`) is byte-for-byte unchanged —
/// it derives the same pubkey/address and hashes the same unsigned bytes as before.
fn ela_send_with(signer: ElaSigner, to: &str, amount_ela: &str) -> Result<Value, String> {
    let amount = ela_str_to_sela(amount_ela)?;
    if amount <= 0 {
        return Err("amount must be positive".into());
    }
    let pubkey = signer.pubkey()?;
    let from_ph = did::ela_program_hash(&pubkey);
    let to_ph = did::ela_address_to_program_hash(to)?; // validates the recipient
    let addr = did::ela_address_from_pubkey(&pubkey);

    let utxos = list_unspent(&addr)?;
    let need = amount.saturating_add(FEE_SELA);
    let mut chosen: Vec<Utxo> = Vec::new();
    let mut sum: i64 = 0;
    for u in utxos {
        sum += u.value_sela;
        chosen.push(u);
        if sum >= need {
            break;
        }
    }
    if sum < need {
        return Err(format!("insufficient ELA — need {} sela (amount + fee), have {sum}", need));
    }

    let mut outputs: Vec<([u8; 21], i64)> = vec![(to_ph, amount)];
    let change = sum - need;
    if change > 0 {
        outputs.push((from_ph, change)); // change back to self
    }

    let mut nonce = [0u8; 8];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("nonce: {e}"))?;

    let unsigned = serialize_unsigned(&chosen, &outputs, &nonce)?;
    // Local hashes the unsigned bytes here; Ledger hashes them on the device. Both
    // produce a 64-byte low-S r‖s over sha256(unsigned).
    let sig = signer.sign_unsigned(&unsigned)?;
    let signed = serialize_signed(&unsigned, &sig, &pubkey);
    let raw = hex_lower(&signed);

    let res = rpc("sendrawtransaction", json!([raw])).and_then(|r| {
        r.as_str()
            .map(str::to_string)
            .ok_or_else(|| "sendrawtransaction: no txid".to_string())
    });
    // On a broadcast failure, log the raw signed tx hex so a chain-side REJECTION (bad
    // signature/program) can be dissected offline vs a transient network error.
    if res.is_err() {
        log::warn!("[ela] broadcast FAILED via={} raw_tx={raw}", signer.via());
    }
    // Audited at the signer, like every money path (guard.rs).
    crate::guard::audit(
        "wallet.send",
        json!({
            "chain": "ela-mainchain",
            "via": signer.via(),
            "from": addr,
            "to": to,
            "amount": amount_ela,
            "result": match &res {
                Ok(txid) => json!({ "txHash": txid }),
                Err(e) => json!({ "error": e }),
            },
        }),
    );
    res.map(|txid| json!({ "txHash": txid, "from": addr }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // GOLDEN: the SDK's own P-256 signer vector (Elastos.ELA.Wallet.JS.SDK
    // standard-wallet.test.ts). Proves our derivation + signer are byte-exact.
    #[test]
    fn signer_golden() {
        let m = "cloth always junk crash fun exist stumble shift over benefit fun toe";
        let (priv_bytes, pubkey) = did::derive_p256(m, 0).unwrap();
        assert_eq!(hex_lower(&pubkey), "031f56955cc005122f11cec5264ea5968240a90f01434fb0a1b7429be4b9157d46");
        let digest: [u8; 32] = rev_noflip("88486f91981d11adf53c327e7ab2556b00c8f89b18f56eab8ff72f940c6d8889");
        let sig = ela_sign(&priv_bytes, &digest).unwrap();
        assert_eq!(
            hex_lower(&sig),
            "50cdc759396d1c229852f373d985abb06283c72153032e4b9716dfe426c94cfb45ca4807f6aa6930ef404d631afaaef5c0be48acfddb624a3990e19958aef646"
        );
    }

    #[test]
    fn varuint_and_prefix() {
        let mut v = Vec::new();
        write_varuint(&mut v, 0x40);
        assert_eq!(v, vec![0x40]);
        v.clear();
        write_varuint(&mut v, 0xfd);
        assert_eq!(v, vec![0xfd, 0xfd, 0x00]);
        // unsigned tx starts with the v9 prefix 09 02 00
        let u = serialize_unsigned(
            &[Utxo { txid_display: "00".repeat(32), index: 0, value_sela: 1 }],
            &[([0x21u8; 21], 1)],
            &[1, 2, 3, 4],
        )
        .unwrap();
        assert_eq!(&u[..3], &[0x09, 0x02, 0x00]);
    }

    #[test]
    fn ela_units() {
        assert_eq!(ela_str_to_sela("1.50714932").unwrap(), 150714932);
        assert_eq!(ela_str_to_sela("79808.55818316").unwrap(), 7980855818316);
        assert_eq!(format_ela(150714932), "1.50714932");
        assert_eq!(format_ela(100000000), "1");
    }

    fn rev_noflip(h: &str) -> [u8; 32] {
        hex_decode(h).unwrap().try_into().unwrap()
    }

    // Lock the program layout that the ELA node requires: the signature parameter must be
    // varbytes(0x40 ‖ sig) → `41 40 <64-byte sig>`, and the code varbytes(0x21‖pubkey‖0xAC)
    // → `23 21 <33-byte pubkey> ac`. A missing 0x40 push opcode = "invalid signature length".
    #[test]
    fn signed_program_layout() {
        let unsigned = [0xABu8; 4];
        let sig = [0x11u8; 64];
        let pubkey = [0x02u8; 33];
        let signed = serialize_signed(&unsigned, &sig, &pubkey);
        // After the unsigned bytes: program count, then the parameter varbytes.
        let p = &signed[unsigned.len()..];
        assert_eq!(p[0], 0x01, "one program");
        assert_eq!(p[1], 0x41, "parameter varbytes length = 65 (0x40 push + 64 sig)");
        assert_eq!(p[2], 0x40, "push-64 opcode");
        assert_eq!(&p[3..3 + 64], &sig, "the 64-byte signature");
        // code: varbytes length 0x23, then 0x21 ‖ pubkey ‖ 0xAC.
        let c = &p[3 + 64..];
        assert_eq!(c[0], 0x23, "code varbytes length = 35");
        assert_eq!(c[1], 0x21, "redeem prefix");
        assert_eq!(&c[2..2 + 33], &pubkey);
        assert_eq!(c[35], 0xAC, "checksig trailer");
    }
}
