//! did:elastos (EID sidechain) + ELA mainchain address — derived from the SAME
//! BIP39 mnemonic as the Hey identity + ESC wallet, so Hey shows the exact DID +
//! wallet a user recovers in official Elastos Essentials. One recovery phrase is
//! the single root for everything (see identity.rs / wallet.rs).
//!
//! THE TRAP (verified against the official Java AND JS DID SDKs — the JS one is
//! what Essentials ships): Elastos runs BIP32 over **secp256r1 / NIST P-256**, NOT
//! secp256k1. They forked bitcoinj and swapped the curve, so a standard secp256k1
//! BIP32 derivation produces a DIFFERENT (wrong) DID/address for the same seed.
//!
//! Path: m/44'/0'/0'/0/index  (coin type 0', NOT SLIP-44 2305). index 0 = default.
//! One P-256 key yields BOTH identifiers, differing only by two bytes:
//!   did:elastos   redeem-trailer 0xAD, version 0x67  -> "did:elastos:i…"
//!   ELA mainchain redeem-trailer 0xAC, version 0x21  -> "E…"
//! identifier = Base58( version || ripemd160(sha256(redeem)) || dsha256(payload)[..4] )
//! where redeem = 0x21 || compressed_pubkey(33) || trailer.

use hmac::{Hmac, Mac};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::elliptic_curve::PrimeField; // Scalar::from_repr
use ripemd::Ripemd160;
use sha2::{Digest, Sha256, Sha512};

const HARDENED: u32 = 0x8000_0000;
/// m/44'/0'/0'/0/0 — 44',0',0' hardened; change=0, index=0 non-hardened.
const PATH: [u32; 5] = [44 | HARDENED, HARDENED, HARDENED, 0, 0];

struct ExtKey {
    k: p256::Scalar,
    chain: [u8; 32],
}

/// Parse 32 bytes as a P-256 scalar strictly (>= n is a BIP32-invalid key — for a
/// fixed path the odds are ~2^-128, so we surface it as an error rather than reduce,
/// matching the SDK's "skip invalid" rather than silently diverging).
fn scalar_strict(b: &[u8]) -> Result<p256::Scalar, String> {
    let fb = p256::FieldBytes::clone_from_slice(b);
    Option::<p256::Scalar>::from(p256::Scalar::from_repr(fb)).ok_or_else(|| "scalar out of range".into())
}

fn compressed_pub(k: &p256::Scalar) -> [u8; 33] {
    let pt = (p256::ProjectivePoint::GENERATOR * k).to_affine();
    let ep = pt.to_encoded_point(true); // compressed: 0x02/0x03 || X(32)
    let mut out = [0u8; 33];
    out.copy_from_slice(ep.as_bytes());
    out
}

fn master(seed: &[u8]) -> Result<ExtKey, String> {
    let mut mac = <Hmac<Sha512>>::new_from_slice(b"Bitcoin seed").map_err(|e| e.to_string())?;
    mac.update(seed);
    let i = mac.finalize().into_bytes();
    let mut chain = [0u8; 32];
    chain.copy_from_slice(&i[32..]);
    Ok(ExtKey { k: scalar_strict(&i[..32])?, chain })
}

fn ckd(parent: &ExtKey, index: u32) -> Result<ExtKey, String> {
    let mut mac = <Hmac<Sha512>>::new_from_slice(&parent.chain).map_err(|e| e.to_string())?;
    if index >= HARDENED {
        mac.update(&[0u8]);
        mac.update(parent.k.to_bytes().as_slice()); // ser256(kpar)
    } else {
        mac.update(&compressed_pub(&parent.k)); // serP(point(kpar))
    }
    mac.update(&index.to_be_bytes());
    let i = mac.finalize().into_bytes();
    let il = scalar_strict(&i[..32])?;
    let mut chain = [0u8; 32];
    chain.copy_from_slice(&i[32..]);
    Ok(ExtKey { k: il + parent.k, chain }) // child scalar = (IL + kpar) mod n
}

/// Compressed P-256 public key at m/44'/0'/0'/0/index for this mnemonic.
fn derive_pubkey(mnemonic: &str, index: u32) -> Result<[u8; 33], String> {
    let m = bip39::Mnemonic::parse(mnemonic.trim()).map_err(|e| format!("bad mnemonic: {e}"))?;
    let seed = m.to_seed(""); // BIP39 default empty passphrase
    let mut key = master(&seed)?;
    let mut path = PATH;
    path[4] = index;
    for idx in path {
        key = ckd(&key, idx)?;
    }
    Ok(compressed_pub(&key.k))
}

/// The P-256 private scalar (32B) + compressed pubkey (33B) at m/44'/0'/0'/0/index
/// — the Elastos mainchain SIGNING key (same key as the 'E…' address derivation).
pub fn derive_p256(mnemonic: &str, index: u32) -> Result<([u8; 32], [u8; 33]), String> {
    let m = bip39::Mnemonic::parse(mnemonic.trim()).map_err(|e| format!("bad mnemonic: {e}"))?;
    let seed = m.to_seed("");
    let mut key = master(&seed)?;
    let mut path = PATH;
    path[4] = index;
    for idx in path {
        key = ckd(&key, idx)?;
    }
    let mut priv_bytes = [0u8; 32];
    priv_bytes.copy_from_slice(key.k.to_bytes().as_slice());
    Ok((priv_bytes, compressed_pub(&key.k)))
}

/// The 21-byte mainchain programHash (0x21 || ripemd160(sha256(0x21||pubkey||0xAC)))
/// — the wire form of the standard single-sig 'E…' address.
pub fn ela_program_hash(pubkey: &[u8; 33]) -> [u8; 21] {
    let mut redeem = Vec::with_capacity(35);
    redeem.push(0x21);
    redeem.extend_from_slice(pubkey);
    redeem.push(0xAC);
    let h = ripemd160(&sha256(&redeem));
    let mut out = [0u8; 21];
    out[0] = 0x21;
    out[1..].copy_from_slice(&h);
    out
}

/// Decode an Elastos 'E…' Base58Check address → 21-byte programHash (checks the
/// checksum). Used to build the recipient output of a mainchain transfer.
pub fn ela_address_to_program_hash(addr: &str) -> Result<[u8; 21], String> {
    let raw = bs58::decode(addr.trim()).into_vec().map_err(|e| format!("base58: {e}"))?;
    if raw.len() != 25 {
        return Err("bad Elastos address length".into());
    }
    let (payload, checksum) = raw.split_at(21);
    if sha256(&sha256(payload))[..4] != checksum[..] {
        return Err("address checksum failed — re-check it".into());
    }
    // Version-byte gate (SB-2): the checksum only proves the bytes are intact, not
    // that this is a SPENDABLE mainchain address. Other Elastos-family addresses
    // share the same Base58Check shape — a DID ('i…', 0x67), cross-chain ('X…'),
    // multisig ('8…', 0x12) — and sending ELA to their programHash is irreversible
    // loss. Standard single-sig mainchain is version 0x21 (see ela_program_hash);
    // accept ONLY that. Multisig (0x12) would need an explicit, separate path.
    if payload[0] != 0x21 {
        return Err("not a standard Elastos mainchain (E…) address — refusing to send".into());
    }
    let mut out = [0u8; 21];
    out.copy_from_slice(payload);
    Ok(out)
}

fn sha256(b: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().into()
}

fn ripemd160(b: &[u8]) -> [u8; 20] {
    let mut h = Ripemd160::new();
    h.update(b);
    h.finalize().into()
}

/// Elastos address: Base58( version || ripemd160(sha256(redeem)) || dsha256[..4] ),
/// redeem = 0x21 || pubkey(33) || trailer.
fn elastos_address(pubkey: &[u8; 33], trailer: u8, version: u8) -> String {
    let mut redeem = Vec::with_capacity(35);
    redeem.push(0x21);
    redeem.extend_from_slice(pubkey);
    redeem.push(trailer);

    let mut payload = Vec::with_capacity(25);
    payload.push(version);
    payload.extend_from_slice(&ripemd160(&sha256(&redeem)));
    let checksum = sha256(&sha256(&payload));
    payload.extend_from_slice(&checksum[..4]);
    bs58::encode(payload).into_string()
}

/// `did:elastos:…` for this mnemonic (default DID, index 0). Instant + offline —
/// publishing to EID is a separate optional step.
pub fn elastos_did(mnemonic: &str) -> Result<String, String> {
    let pk = derive_pubkey(mnemonic, 0)?;
    Ok(format!("did:elastos:{}", elastos_address(&pk, 0xAD, 0x67)))
}

/// ELA mainchain `E…` address for this mnemonic (same key, mainchain prefix/sign).
pub fn ela_mainchain_address(mnemonic: &str) -> Result<String, String> {
    let pk = derive_pubkey(mnemonic, 0)?;
    Ok(elastos_address(&pk, 0xAC, 0x21))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape checks: the derivation runs over P-256 without panicking, and the two
    // version bytes produce the expected leading characters (0x67 -> 'i', 0x21 -> 'E').
    // The byte-for-byte Essentials parity assert is added once the golden vector lands.
    const M: &str = "test test test test test test test test test test test junk";

    #[test]
    fn did_shape() {
        let did = elastos_did(M).unwrap();
        assert!(did.starts_with("did:elastos:i"), "got {did}");
    }

    #[test]
    fn mainchain_shape() {
        let a = ela_mainchain_address(M).unwrap();
        assert!(a.starts_with('E'), "got {a}");
        // ELA mainchain addresses are 34 chars.
        assert!(a.len() >= 33 && a.len() <= 34, "len {} ({a})", a.len());
    }

    #[test]
    fn deterministic() {
        assert_eq!(elastos_did(M).unwrap(), elastos_did(M).unwrap());
        assert_eq!(ela_mainchain_address(M).unwrap(), ela_mainchain_address(M).unwrap());
    }

    // SB-2: the send-recipient decoder must accept ONLY spendable mainchain 'E…'
    // (version 0x21) addresses. A did:elastos 'i…' address (version 0x67) shares
    // the Base58Check shape and a valid checksum, so without the version-byte gate
    // it would be accepted and the ELA sent to an unspendable programHash — gone.
    #[test]
    fn send_decoder_rejects_non_mainchain_addresses() {
        let m = "cloth always junk crash fun exist stumble shift over benefit fun toe";
        let ela = ela_mainchain_address(m).unwrap();
        assert!(ela_address_to_program_hash(&ela).is_ok(), "real E… address must decode");

        let did_addr = elastos_did(m).unwrap();
        let i_addr = did_addr.strip_prefix("did:elastos:").unwrap();
        assert!(i_addr.starts_with('i'));
        assert!(
            ela_address_to_program_hash(i_addr).is_err(),
            "did:elastos i… address must be refused as a send target"
        );
    }

    // GOLDEN PARITY — vectors lifted from the official SDK test suites
    // (Elastos.ELA.Wallet.JS.SDK hdkey.test.ts + Elastos.DID.{Java,JS}.SDK HDKeyTest),
    // cross-confirmed across 3 repos. If these pass, Hey's DID + ELA address match
    // Elastos Essentials byte-for-byte from the same mnemonic.
    #[test]
    fn golden_parity_essentials() {
        let m = "cloth always junk crash fun exist stumble shift over benefit fun toe";
        assert_eq!(ela_mainchain_address(m).unwrap(), "EUL3gVZCdJaj6oRfGfzYu8v41ecZvE1Unz");
        assert_eq!(elastos_did(m).unwrap(), "did:elastos:iY4Ghz9tCuWvB5rNwvn4ngWvthZMNzEA7U");

        let m2 = "service illegal blossom voice three eagle grace agent service average knock round";
        assert_eq!(elastos_did(m2).unwrap(), "did:elastos:iW3HU8fTmwkENeVT9UCEvvg3ddUD5oCxYA");
    }
}
