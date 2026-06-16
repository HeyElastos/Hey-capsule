//! Blocking wallet operations, dispatched on an engine worker. These call the
//! embedded runtime's pure-Rust wallet modules directly (the desktop app is the
//! runtime host, so `wallet_phrase()` resolves the in-process signing seed).
//!
//! Every send mints a one-shot spend grant and redeems it immediately before
//! signing (guard.rs) so the on-device audit log records authorize → redeem →
//! send exactly as the Android app does — the egui confirm screen is the human
//! gate that the Android biometric prompt is there.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use hey_mobile_runtime::{did, guard, mainchain, wallet, wallet_phrase};

/// Format integer base units as an even-length `0x…` hex string. The runtime's
/// hex_decode left-pads odd-length input and hex_lower re-emits even-length, so
/// an odd-length grant binding (e.g. "0x100") would not match the value logged
/// in the audit trail ("0x0100"). Emitting even-length here keeps the grant and
/// the audit log byte-for-byte identical.
fn units_to_hex(units: u128) -> String {
    let h = format!("{units:x}");
    if h.len() % 2 == 1 {
        format!("0x0{h}")
    } else {
        format!("0x{h}")
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The native coin symbol for an EVM chain key.
fn native_symbol(chain: &str) -> &'static str {
    match chain {
        "ethereum" => "ETH",
        _ => "ELA", // ESC native is ELA
    }
}

/// Parse a decimal amount string into integer base units (×10^decimals), with
/// strict validation (digits only, not too many fractional places, non-zero).
pub fn decimal_to_units(amount: &str, decimals: u32) -> Result<u128, String> {
    let s = amount.trim();
    if s.is_empty() {
        return Err("enter an amount".into());
    }
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    if frac_part.len() > decimals as usize {
        return Err(format!("too many decimals (max {decimals})"));
    }
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    if !int_part.bytes().all(|c| c.is_ascii_digit()) || !frac_part.bytes().all(|c| c.is_ascii_digit()) {
        return Err("invalid amount".into());
    }
    let mut frac = frac_part.to_string();
    while frac.len() < decimals as usize {
        frac.push('0');
    }
    let int_v: u128 = int_part.parse().map_err(|_| "amount too large".to_string())?;
    let frac_v: u128 = if frac.is_empty() { 0 } else { frac.parse().map_err(|_| "amount too large".to_string())? };
    let scale = 10u128.checked_pow(decimals).ok_or("decimals too large")?;
    let units = int_v
        .checked_mul(scale)
        .and_then(|v| v.checked_add(frac_v))
        .ok_or("amount too large")?;
    if units == 0 {
        return Err("amount must be greater than zero".into());
    }
    Ok(units)
}

fn tx_record(chain: &str, symbol: &str, to: &str, amount: &str, hash: &str, to_did: &str) -> Value {
    json!({
        "chain": chain,
        "symbol": symbol,
        "to": to,
        "amount": amount,
        "hash": hash,
        "kind": if to_did.is_empty() { "sent" } else { "tip" },
        "ts": now_ms(),
    })
}

/// Build, authorize, sign and broadcast a transfer. Returns a local tx record on
/// success. `token` Some => ERC-20, None => the chain's native coin. `chain` is a
/// key ("esc" | "ethereum" | "ela"). `amount_dec` is a human decimal string.
pub fn send(
    chain: &str,
    token: Option<&Value>,
    to: &str,
    amount_dec: &str,
    to_did: &str,
) -> Result<Value, String> {
    let phrase = wallet_phrase()?;

    if chain == "ela" {
        // ela_send validates the recipient (ela_address_to_program_hash).
        let amt = amount_dec.trim();
        // sanity-parse so we reject garbage before minting a grant
        let _ = decimal_to_units(amt, 8)?;
        let kind = "ela";
        // No hardware spend-binding on desktop (sig_hex=None) — the confirm screen
        // is the gate; the runtime falls back to that when binding isn't enrolled.
        let grant = guard::authorize_spend(kind, to, amt, None)?;
        guard::redeem_spend(&grant, kind, to, amt)?;
        let res = mainchain::ela_send(&phrase, to, amt)?;
        let hash = res
            .get("txHash")
            .or_else(|| res.get("txid"))
            .or_else(|| res.get("hash"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Ok(tx_record("ela", "ELA", to, amt, &hash, to_did));
    }

    // EVM (ESC / Ethereum) — validate + checksum the recipient first.
    let to_checked = wallet::validate_address(to)?;

    if let Some(t) = token {
        let contract = t.get("contract").and_then(Value::as_str).unwrap_or("");
        if contract.is_empty() {
            return Err("token has no contract address".into());
        }
        let decimals = t.get("decimals").and_then(Value::as_u64).unwrap_or(18) as u32;
        let symbol = t.get("symbol").and_then(Value::as_str).unwrap_or("TOKEN");
        let units = decimal_to_units(amount_dec, decimals)?;
        let amount_hex = units_to_hex(units);
        let kind = format!("erc20:{chain}:{contract}");
        let grant = guard::authorize_spend(&kind, &to_checked, &amount_hex, None)?;
        guard::redeem_spend(&grant, &kind, &to_checked, &amount_hex)?;
        let res = wallet::evm_token_send(&phrase, chain, contract, &to_checked, &amount_hex)?;
        let hash = res.get("txHash").and_then(Value::as_str).unwrap_or("").to_string();
        Ok(tx_record(chain, symbol, &to_checked, amount_dec, &hash, to_did))
    } else {
        let units = decimal_to_units(amount_dec, 18)?;
        let value_hex = units_to_hex(units);
        let kind = format!("evm:{chain}");
        let grant = guard::authorize_spend(&kind, &to_checked, &value_hex, None)?;
        guard::redeem_spend(&grant, &kind, &to_checked, &value_hex)?;
        let res = wallet::esc_send(&phrase, chain, &to_checked, &value_hex)?;
        let hash = res.get("txHash").and_then(Value::as_str).unwrap_or("").to_string();
        Ok(tx_record(chain, native_symbol(chain), &to_checked, amount_dec, &hash, to_did))
    }
}

/// Confirmation status of a broadcast EVM tx: "pending" | "success" | "failed".
/// Read-only RPC poll (the receipt) — NOT part of the signing path. ELA mainchain
/// has no equivalent receipt lookup in the engine, so the caller only polls EVM.
pub fn tx_status(chain: &str, hash: &str) -> Result<String, String> {
    let v = wallet::esc_tx_status(chain, hash)?;
    Ok(v.get("status").and_then(Value::as_str).unwrap_or("pending").to_string())
}

/// Early client-side recipient validation BEFORE the Review step (mirrors Android's
/// `checkAddress` / `isElaAddress`). A typo-catch on top of the deeper validation
/// `send()` already does — never a substitute for it. Returns Ok(()) when the
/// address LOOKS valid for the chain, Err(msg) with an inline reason otherwise.
///   EVM ("esc"/"ethereum"/…): full `validate_address` (length + EIP-55 checksum).
///   ELA mainchain: the same cheap shape check Android uses (E… , 33–34 chars) —
///   the authoritative Base58Check/version-byte check still runs inside `send()`.
pub fn precheck_recipient(chain: &str, to: &str) -> Result<(), String> {
    let a = to.trim();
    if a.is_empty() {
        return Err("Enter a recipient address".into());
    }
    if chain == "ela" {
        let n = a.chars().count();
        if !(a.starts_with('E') && (33..=34).contains(&n)) {
            return Err("Enter a valid Elastos mainchain address (starts with E)".into());
        }
        Ok(())
    } else {
        wallet::validate_address(a).map(|_| ())
    }
}

/// Resolve the three wallet addresses + the EVM chain list in one worker hop.
pub fn addresses() -> Result<(String, String, String, Value), String> {
    let p = wallet_phrase()?;
    let evm = wallet::esc_address(&p)?;
    let ela = did::ela_mainchain_address(&p).unwrap_or_default();
    let did_str = did::elastos_did(&p).unwrap_or_default();
    let chains = wallet::evm_chains_json();
    Ok((evm, ela, did_str, chains))
}

/// Fetch a chain's balance bundle. EVM => evm_balances (native + curated tokens);
/// "ela" => the mainchain balance.
pub fn balance(chain: &str) -> Result<Value, String> {
    let p = wallet_phrase()?;
    if chain == "ela" {
        mainchain::ela_balance(&p)
    } else {
        wallet::evm_balances(&p, chain)
    }
}
