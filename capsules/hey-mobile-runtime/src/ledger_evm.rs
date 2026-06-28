//! Ledger Ethereum app (`LedgerHQ/app-ethereum`) — GET_ADDRESS + signTransaction over
//! the BLE transport in [`crate::ledger_ble`]. Covers EVM chains (ESC / ETH / Base).
//! CLA `0xE0`; GET_ADDRESS INS `0x02`, SIGN INS `0x04`.
//!
//! Hey emits LEGACY EIP-155 transactions, and the device hashes exactly the preimage
//! Hey already builds — `rlp([nonce, gasPrice, gas, to, value, data, chainId, 0, 0])` —
//! so we hand it that same preimage and get back `(v, r, s)`. A plain native transfer
//! needs NO blind-signing on the device; only contract/token calls (non-empty `data`)
//! on a chain with no clear-signing plugin require "blind signing / contract data".
//!
//! Path encoding here differs from the Elastos app: it's a PREFIX of
//! `numDerivations(1) ‖ level×4-BE` (vs the ELA app's 20-byte suffix).

use std::time::Duration;

use crate::ledger_ble;

const CLA: u8 = 0xE0;
const INS_GET_ADDRESS: u8 = 0x02;
const INS_SIGN: u8 = 0x04;
const P1_FIRST: u8 = 0x00;
const P1_MORE: u8 = 0x80;
const APDU_TIMEOUT: Duration = Duration::from_secs(60);
const NEG_TIMEOUT: Duration = Duration::from_secs(3);

/// "m/44'/60'/0'/0/0" → `numDerivations(1) ‖ level×4-BE` (hardened sets the high bit).
fn encode_path(path: &str) -> Result<Vec<u8>, String> {
    let p = path.trim().trim_start_matches("m/").trim_start_matches('/');
    let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() || segs.len() > 10 {
        return Err(format!("EVM path must have 1-10 levels, got {}", segs.len()));
    }
    let mut out = vec![segs.len() as u8];
    for s in &segs {
        let hardened = s.ends_with('\'') || s.ends_with('h') || s.ends_with('H');
        let num: u32 = s
            .trim_end_matches(['\'', 'h', 'H'])
            .parse()
            .map_err(|_| format!("bad path segment {s:?}"))?;
        let v = if hardened { num | 0x8000_0000 } else { num };
        out.extend_from_slice(&v.to_be_bytes());
    }
    Ok(out)
}

/// GET_ADDRESS at `path` → (EIP-55 `0x…` address, 33-byte compressed pubkey hex).
/// Requires the Ethereum app open on the device.
pub fn get_address(path: &str) -> Result<(String, String), String> {
    let pb = encode_path(path)?;
    ledger_ble::ensure_negotiated(NEG_TIMEOUT);
    let mut apdu = vec![CLA, INS_GET_ADDRESS, 0x00, 0x00, pb.len() as u8];
    apdu.extend_from_slice(&pb);
    let resp = ledger_ble::exchange(&apdu, APDU_TIMEOUT)?;
    parse_address(&resp)
}

/// Response: `pubLen(1) ‖ pubkey(65) ‖ addrLen(1) ‖ address ASCII(addrLen) [‖ chaincode]`.
/// The address is ASCII hex chars (no 0x). We return it 0x-prefixed + the compressed pubkey.
fn parse_address(resp: &[u8]) -> Result<(String, String), String> {
    if resp.is_empty() {
        return Err("ledger: empty GET_ADDRESS response".into());
    }
    let pub_len = resp[0] as usize;
    if pub_len != 65 || resp.len() < 1 + pub_len + 1 {
        return Err(format!("ledger: bad GET_ADDRESS response ({} bytes)", resp.len()));
    }
    let pubkey = &resp[1..1 + pub_len]; // 0x04 || X(32) || Y(32)
    let addr_len = resp[1 + pub_len] as usize;
    let start = 1 + pub_len + 1;
    let ascii = resp.get(start..start + addr_len).ok_or("ledger: truncated address")?;
    let addr = String::from_utf8(ascii.to_vec()).map_err(|_| "ledger: non-utf8 address")?;
    let compressed = compress(pubkey)?;
    let pk_hex: String = compressed.iter().map(|b| format!("{b:02x}")).collect();
    let addr0x = if addr.starts_with("0x") { addr } else { format!("0x{addr}") };
    Ok((addr0x, pk_hex))
}

fn compress(pk65: &[u8]) -> Result<[u8; 33], String> {
    if pk65.len() != 65 || pk65[0] != 0x04 {
        return Err("ledger: not an uncompressed point".into());
    }
    let mut out = [0u8; 33];
    out[0] = if pk65[64] & 1 == 1 { 0x03 } else { 0x02 };
    out[1..].copy_from_slice(&pk65[1..33]);
    Ok(out)
}

/// signTransaction for a LEGACY EIP-155 tx. `preimage` = the same
/// `rlp([nonce,gasPrice,gas,to,value,data,chainId,0,0])` Hey signs locally; the device
/// keccak256-hashes it and signs. Returns `(v, r, s)` with `v` the FULL EIP-155 value
/// (`chainId*2+35+parity`), reconstructed from the device's single-byte v.
pub fn sign_legacy(path: &str, preimage: &[u8], chain_id: u64) -> Result<(u64, [u8; 32], [u8; 32]), String> {
    let pb = encode_path(path)?;
    ledger_ble::ensure_negotiated(NEG_TIMEOUT);
    let mut payload = pb;
    payload.extend_from_slice(preimage); // path PREFIX, then the unsigned RLP
    // A native transfer fits in one APDU; larger txs chunk at 255 bytes (P1 first/more).
    let chunks: Vec<&[u8]> = payload.chunks(255).collect();
    let mut last = Vec::new();
    for (i, ch) in chunks.iter().enumerate() {
        let p1 = if i == 0 { P1_FIRST } else { P1_MORE };
        let mut apdu = vec![CLA, INS_SIGN, p1, 0x00, ch.len() as u8];
        apdu.extend_from_slice(ch);
        last = ledger_ble::exchange(&apdu, APDU_TIMEOUT)?;
    }
    parse_sig(&last, chain_id)
}

/// Response: `v(1) ‖ r(32) ‖ s(32)`. The device returns `(chainId*2+35+parity) & 0xff`;
/// recover the full EIP-155 v with the known chainId.
fn parse_sig(resp: &[u8], chain_id: u64) -> Result<(u64, [u8; 32], [u8; 32]), String> {
    if resp.len() < 65 {
        return Err(format!("ledger sig: short response ({} bytes)", resp.len()));
    }
    let v_device = resp[0] as u64;
    let mut r = [0u8; 32];
    r.copy_from_slice(&resp[1..33]);
    let mut s = [0u8; 32];
    s.copy_from_slice(&resp[33..65]);
    let base = chain_id.wrapping_mul(2).wrapping_add(35);
    let parity = (v_device + 256 - (base % 256)) % 256; // 0 or 1
    if parity > 1 {
        return Err(format!("ledger sig: unexpected v byte {v_device} for chainId {chain_id}"));
    }
    Ok((base + parity, r, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_prefix_encoding() {
        // m/44'/60'/0'/0/0 → count 5, then 8000002C 8000003C 80000000 00000000 00000000
        let p = encode_path("m/44'/60'/0'/0/0").unwrap();
        assert_eq!(
            p,
            [
                0x05, 0x80, 0x00, 0x00, 0x2C, 0x80, 0x00, 0x00, 0x3C, 0x80, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn pubkey_compress_parity() {
        let mut pk = [0u8; 65];
        pk[0] = 0x04;
        for i in 1..33 {
            pk[i] = i as u8;
        }
        pk[64] = 0x02;
        assert_eq!(compress(&pk).unwrap()[0], 0x02);
        pk[64] = 0x09;
        assert_eq!(compress(&pk).unwrap()[0], 0x03);
    }

    #[test]
    fn parse_address_layout() {
        let mut resp = vec![65u8]; // pubLen
        let mut pk = vec![0x04u8];
        pk.extend((1u8..=64).collect::<Vec<u8>>());
        resp.extend_from_slice(&pk);
        let addr = "52908400098527886e0f7030069857d2e4169ee7"; // 40 ascii hex
        resp.push(addr.len() as u8);
        resp.extend_from_slice(addr.as_bytes());
        let (a, pkhex) = parse_address(&resp).unwrap();
        assert_eq!(a, format!("0x{addr}"));
        assert_eq!(pkhex.len(), 66); // 33 bytes hex
    }

    #[test]
    fn v_reconstruction() {
        // chainId 20 (ESC): base 75 → device 75/76 → v 75/76, parity 0/1.
        let mk = |v: u8| {
            let mut r = vec![v];
            r.extend_from_slice(&[0x11u8; 32]);
            r.extend_from_slice(&[0x22u8; 32]);
            r
        };
        assert_eq!(parse_sig(&mk(75), 20).unwrap().0, 75);
        assert_eq!(parse_sig(&mk(76), 20).unwrap().0, 76);
        // chainId 8453 (Base): base 16941, %256 = 45 → device 45/46 → v 16941/16942.
        assert_eq!(parse_sig(&mk(45), 8453).unwrap().0, 16941);
        assert_eq!(parse_sig(&mk(46), 8453).unwrap().0, 16942);
        // chainId 1 (Ethereum): base 37 → device 37/38.
        assert_eq!(parse_sig(&mk(37), 1).unwrap().0, 37);
        assert_eq!(parse_sig(&mk(38), 1).unwrap().0, 38);
    }
}
