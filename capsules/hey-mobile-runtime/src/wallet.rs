//! On-device Elastos Smart Chain (ESC) wallet — derived from the SAME BIP39
//! recovery phrase as the Hey identity, so one phrase recovers everything in
//! official Elastos Essentials too.
//!
//! Parity contract (verified against Essentials / Ethereum standards):
//!   * derivation : BIP44 `m/44'/60'/0'/0/0` (coin_type 60 = Ethereum)
//!   * passphrase : empty ("") — BIP39 default
//!   * curve      : secp256k1
//!   * address    : keccak256(uncompressed_pubkey[1..])[12..] + EIP-55 checksum
//!   * network    : ESC mainnet, chainId 20 (0x14), RPC https://api.elastos.io/esc
//!
//! The phrase NEVER leaves the phone. Kotlin unseals it from the StrongBox/TEE
//! vault and hands it to these functions; the signing key is reconstructed,
//! used, and dropped. Address + balance are read-only and safe. `esc_send` signs +
//! broadcasts a real native value transfer on the SELECTED EVM chain (symbol +
//! chainId from the registry) — money-critical, gated behind a biometric confirm on
//! the UI side; legacy gasPrice is padded on high-baseFee chains (Ethereum).

use k256::ecdsa::SigningKey;
use serde_json::{json, Value};

/// Plain native-value transfer costs exactly 21000 gas.
const TRANSFER_GAS: u64 = 21000;
const WEI_PER_TOKEN: u128 = 1_000_000_000_000_000_000; // EVM native token = 18 decimals

/// An EVM chain reached via `elastos://<key>/blockchain.data` — resolved here to
/// its Web2-compat https RPC until a real blockchain-data provider exists (the
/// resolver swaps in cleanly later). Adding a chain = one entry. The wallet's EVM
/// address is IDENTICAL across all of them (m/44'/60'/0'/0/0).
pub struct EvmChain {
    pub key: &'static str,
    pub name: &'static str,
    pub chain_id: u64,
    pub rpc: &'static str,
    pub symbol: &'static str,
}

/// A redeemed-on-fee spend grant threaded into the signer: the one-shot token plus
/// the canonical (kind,to,amount) the guard re-checks. The grant is consumed INSIDE
/// `sign_and_send` AFTER the fee (gasPrice*gasLimit) is computed, so a max-fee bound
/// in the grant is enforced against the real fee before signing. `None` = the caller
/// already redeemed up front (legacy / no fee bound).
pub struct SpendRedeem {
    pub token: String,
    pub kind: String,
    pub to: String,
    pub amount: String,
}

/// Per-chain sane ceiling on a single tx's gasPrice (wei) — a lying RPC can't push
/// the legacy gasPrice above this. Generous (covers real spikes) but bounds the
/// fee-drain blast radius. Ethereum (chainId 1) tolerates volatile baseFee; the
/// Elastos sidechains + others are cheap, so a much tighter cap applies.
fn gas_price_ceiling(chain_id: u64) -> u128 {
    match chain_id {
        1 => 3_000 * 1_000_000_000u128,  // Ethereum: 3000 gwei (extreme-spike headroom)
        // Base (L2, gas in ETH): much pricier than the Elastos sidechains, so the
        // cheap-chain 100 gwei default is too tight under L1-congestion spikes — give
        // it its own realistic 50 gwei ceiling (well above its typical sub-gwei fee,
        // bounded vs the ELA chains' true cents-level cost).
        8453 => 50 * 1_000_000_000u128,  // Base: 50 gwei
        _ => 100 * 1_000_000_000u128,    // ESC/EID/other cheap chains: 100 gwei
    }
}

const EVM_CHAINS: &[EvmChain] = &[
    // Elastos sidechains first — both EVM, both reached at the SAME m/44'/60'/0'/0/0
    // address as Essentials. ESC = smart chain (DeFi); EID = identity chain (DID docs).
    EvmChain { key: "esc", name: "Elastos Smart Chain", chain_id: 20, rpc: "https://api.elastos.io/esc", symbol: "ELA" },
    EvmChain { key: "eid", name: "Elastos Identity Chain", chain_id: 22, rpc: "https://api.elastos.io/eid", symbol: "ELA" },
    EvmChain { key: "ethereum", name: "Ethereum", chain_id: 1, rpc: "https://ethereum-rpc.publicnode.com", symbol: "ETH" },
    // Base — Coinbase L2 (EVM, chainId 8453). SAME secp256k1 address as Ethereum; gas paid in ETH.
    EvmChain { key: "base", name: "Base", chain_id: 8453, rpc: "https://mainnet.base.org", symbol: "ETH" },
];

fn evm_chain(key: &str) -> Result<&'static EvmChain, String> {
    let k = key.trim();
    let k = if k.is_empty() { "esc" } else { k };
    EVM_CHAINS.iter().find(|c| c.key == k).ok_or_else(|| format!("unknown chain: {key}"))
}

/// True if `host` is a loopback / RFC1918 private address — the ONLY case where a
/// cleartext `http://` RPC override is tolerated (a node on the user's own LAN,
/// never reachable by an on-path internet attacker). Covers 127.0.0.0/8, ::1,
/// localhost, 10.0.0.0/8, 192.168.0.0/16 and 172.16.0.0/12. `host` is the URL
/// authority with any port already stripped.
fn is_private_host(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    if h.eq_ignore_ascii_case("localhost") || h == "::1" {
        return true;
    }
    let oct: Vec<u8> = h.split('.').filter_map(|p| p.parse::<u8>().ok()).collect();
    if oct.len() != 4 {
        return false; // not a dotted-quad IPv4 → treat as public (fail closed)
    }
    match (oct[0], oct[1]) {
        (127, _) => true,                        // 127.0.0.0/8 loopback
        (10, _) => true,                         // 10.0.0.0/8
        (192, 168) => true,                      // 192.168.0.0/16
        (172, b) if (16..=31).contains(&b) => true, // 172.16.0.0/12
        _ => false,
    }
}

/// Bare host from an http(s) URL (no scheme / userinfo / port). "" if not http(s).
fn url_host(url: &str) -> String {
    let rest = match url.trim().strip_prefix("https://").or_else(|| url.trim().strip_prefix("http://")) {
        Some(r) => r,
        None => return String::new(),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(end) = host_port.strip_prefix('[').and_then(|s| s.split_once(']')) {
        end.0
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    host.to_string()
}

/// The NFT image URL the gallery may AUTO-load, hardened against W6-NFT-PIXEL/SSRF.
/// `ipfs://` images are content-addressed (the same CID for everyone, fetched via the user's
/// gateway — never a per-victim tracking pixel) and `data:` images are inline (no network), so
/// both pass. A RAW http(s) image is an attacker-controllable host: auto-loading an airdropped
/// NFT's image would leak the device's public IP + online-now, so it is stripped to "" (the UI
/// shows a placeholder). Private/loopback/LAN hosts are always blocked (SSRF).
fn safe_nft_image(image_uri: &str) -> String {
    let u = image_uri.trim();
    if u.starts_with("data:") {
        return u.to_string(); // inline, no network
    }
    // Only ipfs:// images auto-load: content-addressed and fetched via the user's OWN gateway
    // (their default public one, or a self-hosted LAN gateway) — never a per-victim tracking
    // pixel, and never an attacker host. A raw http(s) image is attacker-controllable, so it is
    // stripped to "" (the UI shows a placeholder) to kill the zero-click IP/online-now leak.
    if u.starts_with("ipfs://") || u.starts_with("ipfs/") {
        resolve_ipfs(u)
    } else {
        String::new()
    }
}

/// True if an IPv4 is NOT globally routable (the set an SSRF fetch must never reach).
fn v4_is_private(v4: std::net::Ipv4Addr) -> bool {
    v4.is_loopback() || v4.is_private() || v4.is_link_local()
        || v4.is_unspecified() || v4.is_broadcast()
        || v4.octets()[0] == 0 // 0.0.0.0/8 "this network"
        || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64) // CGNAT 100.64.0.0/10
}

/// True if an IP is NOT globally routable (loopback/private/link-local/unspecified/CGNAT) — the
/// set an SSRF fetch must never reach.
fn ip_is_private(ip: std::net::IpAddr) -> bool {
    // Canonicalize FIRST: an IPv4-mapped IPv6 literal (::ffff:127.0.0.1) is a real IPv4 loopback
    // wearing a v6 costume — to_canonical() unwraps it so the v4 arm classifies it. Without this,
    // Ipv6Addr::is_loopback() is false for ::ffff:127.0.0.1 and the address sails through as
    // "public v6" (the exact bypass the verifier found: http://[::ffff:127.0.0.1]:31744).
    match ip.to_canonical() {
        std::net::IpAddr::V4(v4) => v4_is_private(v4),
        std::net::IpAddr::V6(v6) => {
            let seg = v6.segments();
            // NAT64 well-known prefix 64:ff9b::/96 embeds an IPv4 in the low 32 bits; on a NAT64
            // network it routes to that v4 (incl. loopback/private). Extract and classify it.
            if seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2..6] == [0, 0, 0, 0] {
                let o = v6.octets();
                return v4_is_private(std::net::Ipv4Addr::new(o[12], o[13], o[14], o[15]));
            }
            // Deprecated IPv4-compatible form ::a.b.c.d (RFC 4291, high 96 bits zero): the kernel
            // won't route it to loopback today, but classify the embedded v4 anyway for hygiene so
            // the gate never depends on a kernel routing quirk. (::1 / :: fall through to the
            // is_loopback / is_unspecified checks below and are still caught.)
            if seg[..6] == [0, 0, 0, 0, 0, 0] {
                let o = v6.octets();
                if v4_is_private(std::net::Ipv4Addr::new(o[12], o[13], o[14], o[15])) {
                    return true;
                }
            }
            v6.is_loopback() || v6.is_unspecified()
                || (seg[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (seg[0] & 0xfe00) == 0xfc00 // unique-local fc00::/7
        }
    }
}

/// W6-NFT-SSRF (hardened): RESOLVE `host` and reject if it maps to any private/loopback address.
/// Closes the forms the is_private_host string parser misses — decimal/hex/octal/short IPv4
/// literals (the OS resolver normalizes "2130706433" / "0x7f000001" / "127.1" → 127.0.0.1) and a
/// DNS name whose record points at a private host. Fail-closed when it can't resolve.
fn host_resolves_private(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    match (host, 80u16).to_socket_addrs() {
        Ok(addrs) => {
            let v: Vec<std::net::SocketAddr> = addrs.collect();
            v.is_empty() || v.iter().any(|sa| ip_is_private(sa.ip()))
        }
        Err(_) => true,
    }
}

/// W6-NFT-SSRF resolver: ureq dials EXACTLY the SocketAddrs this returns, so vetting them here is
/// the AUTHORITATIVE gate. Returning only the public addresses (or an error when none remain)
/// closes the DNS-rebinding TOCTOU — there is no second, unvetted resolution between the up-front
/// host_resolves_private check and the actual connect, because the connect uses THIS result.
fn public_only_resolver(netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    let public: Vec<std::net::SocketAddr> =
        netloc.to_socket_addrs()?.filter(|sa| !ip_is_private(sa.ip())).collect();
    if public.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "blocked: host resolves only to private/loopback addresses",
        ));
    }
    Ok(public)
}

/// Classify an http(s) override URL for a MONEY/signing chain. Returns Ok(insecure)
/// where `insecure` is true for a tolerated cleartext (loopback/RFC1918) endpoint,
/// or Err for a rejected one. https is always Ok(false). Plain http is allowed ONLY
/// to a private host — a cleartext public RPC lets an on-path attacker read the
/// signed raw tx, inflate the gas price (real fee drain) and forge "confirmed".
fn classify_signing_url(url: &str) -> Result<bool, String> {
    let u = url.trim();
    if u.starts_with("https://") {
        return Ok(false);
    }
    let Some(rest) = u.strip_prefix("http://") else {
        return Err("node URL must start with https:// (or http:// for a node on your own device/LAN)".into());
    };
    // authority = up to the first '/', '?' or '#'; strip userinfo + port.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    // IPv6 literal: [::1]:port — keep the bracketed part; else split host:port.
    let host = if let Some(end) = host_port.strip_prefix('[').and_then(|s| s.split_once(']')) {
        end.0
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    if is_private_host(host) {
        return Ok(true); // tolerated: a node on the user's own device / LAN
    }
    Err("a public RPC node must use https:// — plain http:// is only allowed for a node on your own device or LAN (127.*, 10.*, 192.168.*, 172.16–31.*)".into())
}

/// Per-chain persistent flag (`<chain>-rpc-insecure`) recording that the user's
/// override is cleartext http (loopback/LAN). The UI surfaces it as an INSECURE
/// badge so a self-hoster knows their RPC isn't encrypted. Set/cleared alongside
/// the `<chain>-rpc.txt` file.
fn insecure_flag_path(dir: &std::path::Path, chain: &str) -> std::path::PathBuf {
    dir.join(format!("{chain}-rpc-insecure"))
}

/// True if chain `key`'s active RPC override is the tolerated cleartext (http) kind.
pub(crate) fn rpc_override_insecure(key: &str) -> bool {
    crate::DATA_DIR
        .get()
        .map(|dir| insecure_flag_path(dir, key).exists())
        .unwrap_or(false)
}

/// Resolve the RPC endpoint for chain `key`, honoring a SELF-HOST override file
/// `<data_dir>/<key>-rpc.txt` (a single URL, trimmed) when present. No override →
/// the bundled public default. Lets a user point ANY chain at their OWN node with
/// no rebuild (e.g. write `https://my-esc-node:port` to `esc-rpc.txt`). Keys:
/// `esc`, `eid`, `ethereum` (EVM) and `ela` (mainchain).
///
/// SIGNING-CHAIN SAFETY (H6): for a money/signing chain (esc/eid/ethereum/ela) a
/// cleartext `http://` override is honored ONLY when it points at a loopback /
/// RFC1918 private host; a cleartext PUBLIC URL is rejected here (fall back to the
/// bundled https default) so a planted override file can't silently downgrade a
/// money path to plaintext. https is always honored.
pub(crate) fn rpc_override(key: &str, default: &str) -> String {
    if let Some(dir) = crate::DATA_DIR.get() {
        if let Ok(s) = std::fs::read_to_string(dir.join(format!("{key}-rpc.txt"))) {
            let s = s.trim();
            let is_signing = SELF_HOST_KEYS.contains(&key);
            let honor = if is_signing {
                classify_signing_url(s).is_ok()
            } else {
                s.starts_with("http://") || s.starts_with("https://")
            };
            if honor {
                return s.to_string();
            }
        }
    }
    default.to_string()
}

/// The (possibly user-overridden) RPC URL for an EVM chain.
fn chain_rpc(chain: &EvmChain) -> String {
    rpc_override(chain.key, chain.rpc)
}

// ── NFT indexer + IPFS gateway (self-hostable, mirrors `rpc_override`) ──────
//
// NFT *discovery* (listing every collectible a wallet owns) can't be done over
// plain eth_call — it needs an explorer/index. We default to the open-source
// Blockscout v2 index, but treat its URL as ONE MORE user-overridable endpoint,
// exactly like `rpc_override`: write `<data_dir>/<chain>-nftindex.txt` to point
// at your own instance, or the sentinel `off` for a trustless curated/eth_call-
// only mode. IPFS image resolution gets the same treatment (`ipfs-gateway.txt`).

/// Per-chain default NFT index (Blockscout v2 host root, no trailing slash).
const NFT_INDEX_DEFAULTS: &[(&str, &str)] = &[("esc", "https://esc.elastos.io")];
/// Sentinel override value = indexer-free (curated + manual eth_call only).
const NFT_INDEX_OFF: &str = "off";
/// Default IPFS gateway for NFT image/metadata resolution (overridable).
const IPFS_GATEWAY_DEFAULT: &str = "https://ipfs.io/ipfs/";
/// Self-host key for the gateway override file (`ipfs-gateway.txt`).
const IPFS_GATEWAY_KEY: &str = "ipfs-gateway";

/// The bundled-default NFT index URL for `chain`, or "" if the chain has none.
fn nft_index_default(chain: &str) -> &'static str {
    NFT_INDEX_DEFAULTS.iter().find(|(k, _)| *k == chain).map(|(_, u)| *u).unwrap_or("")
}

/// Resolve the NFT index endpoint for `chain`, honoring `<data_dir>/<chain>-
/// nftindex.txt` (a URL, or the literal `off`). No file → the bundled default.
/// Returns "" when there is no default and no override (no index available),
/// and the literal "off" when the user chose indexer-free mode.
pub(crate) fn nft_index_url(chain: &str) -> String {
    let ov = self_host_value(&format!("{chain}-nftindex"));
    if ov == NFT_INDEX_OFF {
        return NFT_INDEX_OFF.to_string();
    }
    if !ov.is_empty() {
        return ov;
    }
    nft_index_default(chain).to_string()
}

/// The (possibly user-overridden) IPFS gateway, always ending in `/`.
pub(crate) fn ipfs_gateway() -> String {
    let ov = self_host_value(IPFS_GATEWAY_KEY);
    let g = if ov.is_empty() { IPFS_GATEWAY_DEFAULT.to_string() } else { ov };
    if g.ends_with('/') { g } else { format!("{g}/") }
}

/// Rewrite an `ipfs://CID[/path]` (or bare CID) into an http(s) gateway URL.
/// Non-ipfs inputs pass through unchanged.
fn resolve_ipfs(uri: &str) -> String {
    let u = uri.trim();
    let cid_path = u
        .strip_prefix("ipfs://ipfs/")
        .or_else(|| u.strip_prefix("ipfs://"))
        .or_else(|| u.strip_prefix("ipfs/"));
    match cid_path {
        Some(rest) => format!("{}{}", ipfs_gateway(), rest),
        None => u.to_string(),
    }
}

/// Self-hostable chains the user may repoint at their OWN node. `esc`/`eid`/
/// `ethereum` are the EVM chains; `ela` is the mainchain. The set must match the
/// keys `rpc_override` reads (`<key>-rpc.txt`).
const SELF_HOST_KEYS: &[&str] = &["esc", "eid", "ethereum", "base", "ela"];

/// WRITE side of the self-host model `rpc_override` reads. Persists (or clears) the
/// `<data_dir>/<chain>-rpc.txt` override that points a chain at the user's own node.
/// An empty `url` removes the file → the chain reverts to its bundled public default.
/// A non-empty `url` must be http(s). `chain` must be one of `esc/eid/ethereum/ela`.
pub(crate) fn set_rpc_override(chain: &str, url: &str) -> Result<(), String> {
    let chain = chain.trim();
    // The NFT-index rows (`<chain>-nftindex`) and the IPFS gateway (`ipfs-gateway`)
    // ride the SAME settings UI + file model as the RPC nodes, but with their own
    // validation: an index may be `off` (trustless mode); a gateway must be http(s).
    let is_nft_index = chain.ends_with("-nftindex")
        && NFT_INDEX_DEFAULTS.iter().any(|(k, _)| chain == format!("{k}-nftindex"));
    let is_ipfs_gw = chain == IPFS_GATEWAY_KEY;
    if !SELF_HOST_KEYS.contains(&chain) && !is_nft_index && !is_ipfs_gw {
        return Err(format!("unknown chain: {chain}"));
    }
    let dir = crate::DATA_DIR.get().ok_or("runtime not ready")?;
    // RPC chains use `<chain>-rpc.txt`; the index/gateway rows use `<name>.txt`.
    let path = if is_nft_index || is_ipfs_gw {
        dir.join(format!("{chain}.txt"))
    } else {
        dir.join(format!("{chain}-rpc.txt"))
    };
    let url = url.trim();
    if url.is_empty() {
        // Revert to the bundled default — best-effort remove (absent file is fine).
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(insecure_flag_path(dir, chain)); // clear any INSECURE badge
        return Ok(());
    }
    // The NFT index alone accepts the `off` sentinel (curated/eth_call only mode).
    if is_nft_index && url.eq_ignore_ascii_case(NFT_INDEX_OFF) {
        return std::fs::write(&path, NFT_INDEX_OFF).map_err(|e| format!("save node: {e}"));
    }
    // Signing/money chains (esc/eid/ethereum/ela): require https for a PUBLIC node;
    // tolerate cleartext http ONLY for loopback/RFC1918 and record an INSECURE flag
    // (H6). Read-only rows (NFT index / IPFS gateway) keep the lax http(s) rule.
    let is_signing = SELF_HOST_KEYS.contains(&chain);
    if is_signing {
        let insecure = classify_signing_url(url)?; // Err rejects a cleartext public node
        let flag = insecure_flag_path(dir, chain);
        if insecure {
            let _ = std::fs::write(&flag, "1");
        } else {
            let _ = std::fs::remove_file(&flag);
        }
    } else if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("node URL must start with http:// or https://".into());
    }
    std::fs::write(&path, url).map_err(|e| format!("save node: {e}"))
}

/// Read the current override file contents (trimmed) for a self-hostable chain, or
/// "" when none is set (using the bundled default).
fn rpc_override_value(key: &str) -> String {
    crate::DATA_DIR
        .get()
        .and_then(|dir| std::fs::read_to_string(dir.join(format!("{key}-rpc.txt"))).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Generic read of a `<data_dir>/<name>.txt` self-host override (trimmed), or ""
/// — used by the NFT-index (`<chain>-nftindex`) + IPFS-gateway (`ipfs-gateway`)
/// rows, which share the RPC-override file model but not the `-rpc.txt` suffix.
fn self_host_value(name: &str) -> String {
    crate::DATA_DIR
        .get()
        .and_then(|dir| std::fs::read_to_string(dir.join(format!("{name}.txt"))).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// `[{key,name,default,override}]` for every self-hostable chain — the EVM chains
/// plus the ELA mainchain. `default` is the bundled public endpoint (the UI shows
/// it as the placeholder); `override` is the current self-host file, or "" if the
/// chain is on the default. Drives the "Blockchain nodes" settings UI.
pub(crate) fn rpc_nodes_json() -> Value {
    let mut nodes: Vec<Value> = EVM_CHAINS
        .iter()
        .map(|c| {
            json!({
                "key": c.key,
                "name": c.name,
                "default": c.rpc,
                "override": rpc_override_value(c.key),
                "insecure": rpc_override_insecure(c.key),
            })
        })
        .collect();
    nodes.push(json!({
        "key": "ela",
        "name": "ELA Mainchain",
        "default": crate::mainchain::default_rpc(),
        "override": rpc_override_value("ela"),
        "insecure": rpc_override_insecure("ela"),
    }));
    // NFT index rows (one per chain that has a default index) — same self-host UI.
    // `off` in the field = trustless curated/eth_call-only mode (no third-party index).
    for (chain, def) in NFT_INDEX_DEFAULTS {
        nodes.push(json!({
            "key": format!("{chain}-nftindex"),
            "name": format!("{} NFT index", chain.to_uppercase()),
            "default": *def,
            "override": self_host_value(&format!("{chain}-nftindex")),
        }));
    }
    // IPFS gateway row (NFT image/metadata resolution) — overridable + self-hostable.
    nodes.push(json!({
        "key": IPFS_GATEWAY_KEY,
        "name": "IPFS gateway (NFT media)",
        "default": IPFS_GATEWAY_DEFAULT,
        "override": self_host_value(IPFS_GATEWAY_KEY),
    }));
    Value::Array(nodes)
}

/// `[{key,name,chainId,symbol}]` — the registered EVM chains, for the wallet UI.
pub fn evm_chains_json() -> Value {
    Value::Array(EVM_CHAINS.iter().map(|c| json!({
        "key": c.key, "name": c.name, "chainId": c.chain_id, "symbol": c.symbol,
    })).collect())
}

/// A curated ERC-20 on a given EVM chain. Curated (not auto-discovered) so random
/// airdropped scam tokens never appear; contracts verified against Etherscan. Adding
/// a token = one row. Addresses are EIP-55 checksummed.
pub struct Erc20 {
    pub chain: &'static str, // EvmChain.key
    pub symbol: &'static str,
    pub name: &'static str,
    pub contract: &'static str,
    pub decimals: u32,
}

const TOKENS: &[Erc20] = &[
    Erc20 { chain: "ethereum", symbol: "USDT", name: "Tether USD", contract: "0xdAC17F958D2ee523a2206206994597C13D831ec7", decimals: 6 },
    Erc20 { chain: "ethereum", symbol: "USDC", name: "USD Coin", contract: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", decimals: 6 },
    Erc20 { chain: "ethereum", symbol: "DAI", name: "Dai Stablecoin", contract: "0x6B175474E89094C44Da98b954EedeAC495271d0F", decimals: 18 },
    // Base stablecoins (6 decimals, verified on BaseScan). USDC is NATIVE (Circle); "USDT" on
    // Base is a BRIDGED token (NOT Tether-issued) — labeled so users aren't misled.
    Erc20 { chain: "base", symbol: "USDC", name: "USD Coin", contract: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913", decimals: 6 },
    Erc20 { chain: "base", symbol: "USDT", name: "Tether USD (bridged)", contract: "0xfde4C96c8593536E31F229EA8f37b2ADa2699bb2", decimals: 6 },
];

// ── key derivation ───────────────────────────────────────────────────────

/// Reconstruct the ESC signing key from the BIP39 recovery phrase via the
/// Essentials-compatible path. We take the 32-byte private scalar from bip32
/// (`XPrv::to_bytes`) rather than its typed key, so a k256 version skew between
/// bip32 and our dep can never silently change the address.
fn signing_key(phrase: &str) -> Result<SigningKey, String> {
    let m = bip39::Mnemonic::parse(phrase.trim()).map_err(|e| format!("bad recovery phrase: {e}"))?;
    // Wipe the BIP39 seed + derived private scalar from the heap on drop (L: seed/
    // key material not zeroized). `Zeroizing<[u8;N]>` derefs to `[u8;N]`, so every
    // `&`/derivation below is byte-identical — only the wipe-on-drop is added.
    let seed = zeroize::Zeroizing::new(m.to_seed("")); // BIP39 default empty passphrase
    let path: bip32::DerivationPath = "m/44'/60'/0'/0/0".parse().map_err(|e| format!("path: {e}"))?;
    let xprv = bip32::XPrv::derive_from_path(seed.as_slice(), &path).map_err(|e| format!("derive: {e}"))?;
    let bytes = zeroize::Zeroizing::new(xprv.to_bytes()); // [u8; 32] private scalar
    SigningKey::from_slice(bytes.as_slice()).map_err(|e| format!("signing key: {e}"))
}

fn address_bytes(sk: &SigningKey) -> [u8; 20] {
    let point = sk.verifying_key().to_encoded_point(false); // 0x04 || X(32) || Y(32)
    let hash = keccak256(&point.as_bytes()[1..]);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[12..]);
    addr
}

/// EIP-55 mixed-case checksum address ("0x"-prefixed).
fn to_checksum(addr: &[u8; 20]) -> String {
    let hex: String = addr.iter().map(|b| format!("{b:02x}")).collect();
    let hash = keccak256(hex.as_bytes());
    let mut out = String::from("0x");
    for (i, c) in hex.chars().enumerate() {
        if c.is_ascii_digit() {
            out.push(c);
        } else {
            let nibble = (hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0xf;
            out.push(if nibble >= 8 { c.to_ascii_uppercase() } else { c });
        }
    }
    out
}

/// The wallet's ESC address for this phrase (EIP-55). Same as Essentials.
pub fn esc_address(phrase: &str) -> Result<String, String> {
    Ok(to_checksum(&address_bytes(&signing_key(phrase)?)))
}

/// Validate + normalize a recipient address. Rejects wrong length, the zero/burn
/// address, and — when the input is mixed-case (so it carries an EIP-55 checksum)
/// — a checksum that doesn't match (i.e. a typo). All-lower/all-upper input has no
/// checksum to verify and is accepted (and returned checksummed). On success the
/// canonical EIP-55 address is returned.
pub fn validate_address(addr: &str) -> Result<String, String> {
    let a = addr.trim();
    let body = a.strip_prefix("0x").or_else(|| a.strip_prefix("0X"))
        .ok_or("address must start with 0x")?;
    if body.len() != 40 || !body.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("address must be 0x followed by 40 hex characters".into());
    }
    let bytes = hex_decode(body)?;
    if bytes.iter().all(|b| *b == 0) {
        return Err("that is the zero address — funds sent there are burned".into());
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    let checksum = to_checksum(&arr);
    let has_upper = body.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = body.chars().any(|c| c.is_ascii_lowercase());
    if has_upper && has_lower && checksum != format!("0x{body}") {
        return Err("address checksum failed — re-check it, a character may be wrong".into());
    }
    Ok(checksum)
}

/// Confirmation status of a broadcast tx on `chain`: "pending" | "success" | "failed".
pub fn esc_tx_status(chain: &str, hash: &str) -> Result<Value, String> {
    let c = evm_chain(chain)?;
    let r = rpc(&chain_rpc(c), "eth_getTransactionReceipt", json!([hash]))?;
    if r.is_null() {
        return Ok(json!({ "status": "pending" }));
    }
    let status = match r.get("status").and_then(Value::as_str) {
        Some("0x1") => "success",
        Some("0x0") => "failed",
        _ => "pending",
    };
    Ok(json!({ "status": status, "block": r.get("blockNumber").cloned().unwrap_or(Value::Null) }))
}

// ── balance (read-only) ──────────────────────────────────────────────────

/// `{ "address", "wei", "balance", "symbol" }` — native balance on `chain` via eth_getBalance.
pub fn esc_balance(phrase: &str, chain: &str) -> Result<Value, String> {
    let c = evm_chain(chain)?;
    let addr = esc_address(phrase)?;
    let wei = balance_wei(&chain_rpc(c), &addr)?;
    Ok(json!({ "address": addr, "wei": wei.to_string(), "balance": format_token(wei), "symbol": c.symbol }))
}

/// The effective per-tx gasPrice (wei) the signer will use on `chain`: the RPC's
/// eth_gasPrice, padded for baseFee growth, then clamped to the per-chain ceiling.
/// Shared by the fee estimate and the signer so the confirm dialog's number matches.
fn effective_gas_price(c: &EvmChain) -> Result<u128, String> {
    let raw = u128_from_hex(&rpc_str(&chain_rpc(c), "eth_gasPrice", json!([]))?)?;
    if raw == 0 {
        return Err("network returned a zero gas price — please try again in a moment".into());
    }
    let ceiling = gas_price_ceiling(c.chain_id);
    let g = raw.min(ceiling);
    let g = if c.chain_id == 1 { g.saturating_mul(2) } else { g.saturating_mul(9) / 8 };
    Ok(g.min(ceiling))
}

/// The gas LIMIT this runtime will sign with for a (to,value,data) call: the RPC
/// `eth_estimateGas` for the REAL call, clamped to a sane ceiling and padded +25%,
/// never below the 21000 native floor. Shared by `esc_fee_estimate` (the confirm
/// dialog + the max-fee bound) and `sign_and_send` (the signer) so the number the
/// user confirms is EXACTLY the number the signer enforces — no contract-recipient
/// drift (M-1). `from` is the sender address; an estimate failure is surfaced
/// (a recipient that reverts must fail the estimate, not silently fall back to 21000).
fn estimate_gas_limit(c: &EvmChain, from: &str, to_param: &str, value_param: &str, data_param: &str) -> Result<u64, String> {
    let est = u64_from_hex(&rpc_str(&chain_rpc(c), "eth_estimateGas", json!([{
        "from": from, "to": to_param, "value": value_param, "data": data_param
    }])).map_err(|e| format!("couldn't estimate gas (recipient may reject this transfer): {e}"))?)?;
    let est = est.min(30_000_000);
    Ok(std::cmp::max(TRANSFER_GAS, est.saturating_add(est / 4))) // +25% headroom
}

/// Estimate the MAX network fee (wei) on `chain` for a native send to `to` (value
/// `value_hex`) for the confirm dialog + the max-fee grant bound. Uses the SAME
/// `eth_estimateGas` gas limit the signer will use (so a CONTRACT recipient that
/// costs more than 21000 gas is reflected here and the grant's max-fee bound won't
/// fail the send closed — M-1) at the effective (padded+clamped) gasPrice.
/// Returns `{ "maxFeeWei", "maxFee", "symbol", "gasPriceWei", "gasLimit" }`.
/// Read-only (the sender address is derived from `phrase`; nothing is signed/sent).
/// The bound the user confirms is `maxFeeWei` verbatim. The per-chain gas_price
/// ceiling clamp is preserved via `effective_gas_price`.
pub fn esc_fee_estimate(phrase: &str, chain: &str, to: &str, value_hex: &str) -> Result<Value, String> {
    let c = evm_chain(chain)?;
    let gp = effective_gas_price(c)?;
    // Derive the sender for an accurate eth_estimateGas (some recipients/precompiles
    // gate on `from`); mirror the signer's native-call shape (empty data).
    let sk = signing_key(phrase)?;
    let from = to_checksum(&address_bytes(&sk));
    let to_bytes = hex_decode(&validate_address(to)?)?;
    let value = be_minimal(&hex_decode(value_hex)?);
    let to_param = format!("0x{}", hex_lower(&to_bytes));
    let value_param = if value.is_empty() { "0x0".to_string() } else { format!("0x{}", hex_lower(&value)) };
    let gas_limit = estimate_gas_limit(c, &from, &to_param, &value_param, "0x")?;
    let max_fee = (gas_limit as u128).saturating_mul(gp);
    Ok(json!({
        "maxFeeWei": max_fee.to_string(),
        "maxFee": format_token(max_fee),
        "gasPriceWei": gp.to_string(),
        "gasLimit": gas_limit.to_string(),
        "symbol": c.symbol,
    }))
}

fn balance_wei(url: &str, addr: &str) -> Result<u128, String> {
    let r = rpc(url, "eth_getBalance", json!([addr, "latest"]))?;
    let h = r.as_str().ok_or("eth_getBalance: not a string")?;
    u128_from_hex(h)
}

/// All balances on `chain` for this mnemonic: the native token + every curated
/// ERC-20 (with its balance, 0 included so the user can see/choose). `{address,
/// tokens:[{symbol,name,contract,decimals,native,balance,raw}]}`.
pub fn evm_balances(phrase: &str, chain: &str) -> Result<Value, String> {
    let c = evm_chain(chain)?;
    let addr = esc_address(phrase)?;
    let native_wei = balance_wei(&chain_rpc(c), &addr).unwrap_or(0);
    let mut tokens = vec![json!({
        "symbol": c.symbol, "name": c.name, "contract": "", "decimals": 18u32, "native": true,
        "balance": format_token(native_wei), "raw": native_wei.to_string(),
    })];
    for t in TOKENS.iter().filter(|t| t.chain == c.key) {
        let bal = erc20_balance_of(&chain_rpc(c), t.contract, &addr).unwrap_or(0);
        tokens.push(json!({
            "symbol": t.symbol, "name": t.name, "contract": t.contract, "decimals": t.decimals, "native": false,
            "balance": format_units(bal, t.decimals), "raw": bal.to_string(),
        }));
    }
    Ok(json!({ "address": addr, "tokens": tokens }))
}

/// ERC-20 balanceOf(addr) via eth_call: selector 0x70a08231 + left-pad32(addr).
fn erc20_balance_of(url: &str, contract: &str, addr: &str) -> Result<u128, String> {
    let a = hex_decode(addr)?;
    let mut data = vec![0x70, 0xa0, 0x82, 0x31];
    data.extend_from_slice(&left_pad32(&a));
    let call = json!([{ "to": contract, "data": format!("0x{}", hex_lower(&data)) }, "latest"]);
    let r = rpc(url, "eth_call", call)?;
    let h = r.as_str().unwrap_or("0x0");
    u128_from_hex(h)
}

/// Format a smallest-unit integer with `decimals` places, trimmed (6 max shown).
fn format_units(raw: u128, decimals: u32) -> String {
    if decimals == 0 {
        return raw.to_string();
    }
    let div = 10u128.checked_pow(decimals).unwrap_or(u128::MAX);
    let whole = raw / div;
    let frac = raw % div;
    if frac == 0 {
        return whole.to_string();
    }
    let mut s = format!("{:0width$}", frac, width = decimals as usize);
    if s.len() > 6 {
        s.truncate(6);
    }
    let s = s.trim_end_matches('0');
    if s.is_empty() { whole.to_string() } else { format!("{whole}.{s}") }
}

// ── NFTs (ERC-721 / ERC-1155) — read + enumerate ───────────────────────────
//
// Display half: list a wallet's collectibles. Two sources, mirroring the RPC
// self-host ethos: (1) the open-source Blockscout v2 index (complete, default),
// (2) a trustless curated + manual eth_call fallback when the index is `off` or
// unreachable. eth_call ALONE cannot list "all NFTs an address owns" (1155 has
// no on-chain enumeration; 721 Enumerable is optional), so the off-mode is
// labelled "tracked collections" in the UI, never "all your NFTs".

/// A curated NFT collection on a given EVM chain — the trustless fallback when
/// the index is off. Like `TOKENS`, but for collectibles. (None bundled yet;
/// the user's manual "+ Add collection" list extends this at runtime.)
pub struct NftCollection {
    pub chain: &'static str,
    pub name: &'static str,
    pub contract: &'static str,
    /// "721" | "1155"
    pub kind: &'static str,
}

const NFT_COLLECTIONS: &[NftCollection] = &[];

/// ERC-165 supportsInterface(bytes4) via eth_call: 0x01ffc9a7 + the interface id
/// left-padded to 32 bytes. False on any error (treated as "not supported").
fn nft_supports_interface(url: &str, contract: &str, iface_be4: [u8; 4]) -> bool {
    let mut data = vec![0x01, 0xff, 0xc9, 0xa7];
    data.extend_from_slice(&left_pad32(&iface_be4));
    let call = json!([{ "to": contract, "data": format!("0x{}", hex_lower(&data)) }, "latest"]);
    match rpc(url, "eth_call", call) {
        Ok(v) => v.as_str().map(|h| u128_from_hex(h).unwrap_or(0) == 1).unwrap_or(false),
        Err(_) => false,
    }
}

/// Decode a single dynamic ABI `string` from an eth_call return: the head holds
/// a 32-byte offset, then [len(32)][utf8 bytes]. CLAMPS the declared length
/// (≤64KB) BEFORE slicing so a hostile return can't OOM/panic the runtime.
fn decode_abi_string(ret_hex: &str) -> Result<String, String> {
    let b = hex_decode(ret_hex)?;
    if b.len() < 64 {
        return Err("abi string: short return".into());
    }
    // offset (we only ever decode a single string, so the offset points at 0x20)
    let off = u128_from_bytes_sat(&b[0..32]) as usize;
    // CHECKED: a hostile contract return can pick `off` near usize::MAX so `off + 32` wraps
    // (release builds have overflow-checks off), slipping past the bound and then panicking on
    // the slice — which unwinds across the extern "system" boundary (no catch_unwind) → app
    // abort whenever a malicious NFT tokenURI/uri is viewed. checked_add fails closed instead.
    let off_end = match off.checked_add(32) {
        Some(e) if e <= b.len() => e,
        _ => return Err("abi string: bad offset".into()),
    };
    let len = u128_from_bytes_sat(&b[off..off_end]) as usize;
    const MAX_ABI_STRING: usize = 64 * 1024;
    if len > MAX_ABI_STRING {
        return Err("abi string: declared length too large".into());
    }
    let start = off_end;
    let end = start.checked_add(len).ok_or("abi string: length overflow")?;
    if end > b.len() {
        return Err("abi string: truncated".into());
    }
    Ok(String::from_utf8_lossy(&b[start..end]).into_owned())
}

/// tokenURI(uint256) [721, 0xc87b56dd] or uri(uint256) [1155, 0x0e89341c] via
/// eth_call. `token_id` is the DECIMAL uint256 string (NEVER routed through u128).
/// For 1155 the spec `{id}` placeholder is substituted with the 64-char lowercase
/// hex of the id.
fn nft_token_uri(url: &str, contract: &str, token_id_dec: &str, is_1155: bool) -> Result<String, String> {
    let id_be = decimal_to_be32(token_id_dec)?;
    let selector: [u8; 4] = if is_1155 { [0x0e, 0x89, 0x34, 0x1c] } else { [0xc8, 0x7b, 0x56, 0xdd] };
    let mut data = selector.to_vec();
    data.extend_from_slice(&id_be);
    let call = json!([{ "to": contract, "data": format!("0x{}", hex_lower(&data)) }, "latest"]);
    let ret = rpc_str(url, "eth_call", call)?;
    let uri = decode_abi_string(&ret)?;
    if is_1155 && uri.contains("{id}") {
        // ERC-1155 metadata URI: substitute {id} with the 64-char lowercase hex id.
        let id_hex: String = id_be.iter().map(|x| format!("{x:02x}")).collect();
        Ok(uri.replace("{id}", &id_hex))
    } else {
        Ok(uri)
    }
}

/// ownerOf(uint256) [721, 0x6352211e] → checksummed address, or error.
fn nft_owner_of(url: &str, contract: &str, token_id_dec: &str) -> Result<String, String> {
    let id_be = decimal_to_be32(token_id_dec)?;
    let mut data = vec![0x63, 0x52, 0x21, 0x1e];
    data.extend_from_slice(&id_be);
    let call = json!([{ "to": contract, "data": format!("0x{}", hex_lower(&data)) }, "latest"]);
    let ret = hex_decode(&rpc_str(url, "eth_call", call)?)?;
    if ret.len() < 32 {
        return Err("ownerOf: short return".into());
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&ret[12..32]);
    Ok(to_checksum(&addr))
}

/// ERC-1155 balanceOf(addr,id) [0x00fdd58e] → owned count (clamped to u128).
fn nft_balance_1155(url: &str, contract: &str, addr: &str, token_id_dec: &str) -> Result<u128, String> {
    let a = hex_decode(addr)?;
    let id_be = decimal_to_be32(token_id_dec)?;
    let mut data = vec![0x00, 0xfd, 0xd5, 0x8e];
    data.extend_from_slice(&left_pad32(&a));
    data.extend_from_slice(&id_be);
    let call = json!([{ "to": contract, "data": format!("0x{}", hex_lower(&data)) }, "latest"]);
    let r = rpc(url, "eth_call", call)?;
    u128_from_hex(r.as_str().unwrap_or("0x0"))
}

/// Fetch + parse token metadata (name + image) from a token URI. http(s)/ipfs
/// JSON; the image is rewritten through the user's IPFS gateway. Best-effort —
/// returns blanks on any failure (display-only, never blocks).
fn nft_resolve_metadata(uri: &str) -> (String, String) {
    let url = resolve_ipfs(uri);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return (String::new(), String::new());
    }
    match http_get_json(&url) {
        Ok(v) => {
            let name = v.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let image = v
                .get("image")
                .or_else(|| v.get("image_url"))
                .and_then(Value::as_str)
                .map(safe_nft_image) // W6-NFT-PIXEL: only ipfs/data auto-load; raw remote host → blank
                .unwrap_or_default();
            (name, image)
        }
        Err(_) => (String::new(), String::new()),
    }
}

/// Verify + describe a manually-added NFT the user claims to own on `chain` —
/// the trustless "+ Add collection" path for an id the blind enumeration can't
/// find (a non-Enumerable 721, or any 1155). Probes ownership (721 `ownerOf` ==
/// me, or 1155 `balanceOf(me,id) > 0`), then resolves name+image via tokenURI.
/// `{owned, kind, amount, name, image}` or `{error}`.
pub fn evm_nft_lookup(phrase: &str, chain: &str, contract: &str, token_id_dec: &str) -> Result<Value, String> {
    let c = evm_chain(chain)?;
    let me = esc_address(phrase)?;
    let url = chain_rpc(c);
    if hex_decode(contract)?.len() != 20 {
        return Err("bad NFT contract".into());
    }
    let is_1155 = nft_supports_interface(&url, contract, [0xd9, 0xb6, 0x7a, 0x26]);
    let (owned, amount) = if is_1155 {
        let bal = nft_balance_1155(&url, contract, &me, token_id_dec).unwrap_or(0);
        (bal > 0, bal.to_string())
    } else {
        let owner = nft_owner_of(&url, contract, token_id_dec).unwrap_or_default();
        (owner.eq_ignore_ascii_case(&me), "1".to_string())
    };
    let (mname, image) = nft_token_uri(&url, contract, token_id_dec, is_1155)
        .map(|uri| nft_resolve_metadata(&uri))
        .unwrap_or_default();
    let kind = if is_1155 { "1155" } else { "721" };
    let name = if mname.is_empty() { format!("#{token_id_dec}") } else { mname };
    Ok(json!({ "owned": owned, "kind": kind, "amount": amount, "name": name, "image": image }))
}

/// Enumerate a wallet's NFTs on `chain`. With an index URL (default) this is the
/// Blockscout v2 `/addresses/{addr}/nft` list mapped to a uniform shape. When the
/// index is `off` (or absent), it falls back to curated + `added_contracts`
/// enumeration via eth_call — labelled "tracked collections" client-side.
///
/// Shape: `{address, mode:"index"|"tracked", collections:[{name,contract,kind,
/// instances:[{id,name,image,amount}]}]}`.
pub fn evm_nfts(phrase: &str, chain: &str, added_contracts: &[String]) -> Result<Value, String> {
    let c = evm_chain(chain)?;
    let addr = esc_address(phrase)?;
    let index = nft_index_url(c.key);

    if !index.is_empty() && index != NFT_INDEX_OFF {
        if let Ok(v) = nfts_from_index(&index, &addr) {
            return Ok(json!({ "address": addr, "mode": "index", "collections": v }));
        }
        // Index unreachable → fall through to the trustless eth_call path so the
        // user still sees their tracked collections instead of a hard failure.
    }

    let rpc_url = chain_rpc(c);
    let collections = nfts_from_chain(&rpc_url, c.key, &addr, added_contracts);
    Ok(json!({ "address": addr, "mode": "tracked", "collections": collections }))
}

/// Map the Blockscout v2 NFT list into our uniform per-collection shape.
fn nfts_from_index(index: &str, addr: &str) -> Result<Vec<Value>, String> {
    let url = format!("{}/api/v2/addresses/{}/nft?type=ERC-721,ERC-1155", index.trim_end_matches('/'), addr);
    let v = http_get_json(&url)?;
    let items = v.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    // Group instances by contract.
    let mut by_contract: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for o in items {
        let token = o.get("token");
        let contract = token.and_then(|t| t.get("address")).and_then(Value::as_str).unwrap_or("").to_string();
        if contract.is_empty() {
            continue;
        }
        let kind = match token.and_then(|t| t.get("token_type")).and_then(Value::as_str) {
            Some("ERC-1155") => "1155",
            _ => "721",
        };
        let coll_name = token.and_then(|t| t.get("name")).and_then(Value::as_str).unwrap_or("").to_string();
        let id = o.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let meta = o.get("metadata");
        let name = meta
            .and_then(|m| m.get("name"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| if coll_name.is_empty() { format!("#{id}") } else { format!("{coll_name} #{id}") });
        let image = o
            .get("image_url")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| meta.and_then(|m| m.get("image")).and_then(Value::as_str))
            .map(safe_nft_image) // W6-NFT-PIXEL: enumeration path too — ipfs/data only, raw remote stripped
            .unwrap_or_default();
        let amount = o.get("value").and_then(Value::as_str).unwrap_or("1").to_string();
        let entry = by_contract.entry(contract.clone()).or_insert_with(|| {
            json!({ "name": coll_name, "contract": contract, "kind": kind, "instances": [] })
        });
        if let Some(arr) = entry.get_mut("instances").and_then(Value::as_array_mut) {
            arr.push(json!({ "id": id, "name": name, "image": image, "amount": amount }));
        }
    }
    Ok(by_contract.into_values().collect())
}

/// Trustless fallback: enumerate curated + user-added collections via eth_call.
/// Best-effort per contract; a contract that doesn't enumerate (no Enumerable
/// extension, 1155 without a known id) simply contributes nothing.
fn nfts_from_chain(url: &str, chain: &str, addr: &str, added: &[String]) -> Vec<Value> {
    let mut contracts: Vec<String> =
        NFT_COLLECTIONS.iter().filter(|n| n.chain == chain).map(|n| n.contract.to_string()).collect();
    for ct in added {
        let ct = ct.trim().to_string();
        if !ct.is_empty() && !contracts.iter().any(|c| c.eq_ignore_ascii_case(&ct)) {
            contracts.push(ct);
        }
    }
    let mut out = Vec::new();
    for contract in contracts {
        if let Some(coll) = enumerate_collection(url, &contract, addr) {
            out.push(coll);
        }
    }
    out
}

/// Enumerate the caller's tokens in one 721-Enumerable collection. Returns None
/// when the contract isn't 721, doesn't implement Enumerable, or holds none.
fn enumerate_collection(url: &str, contract: &str, addr: &str) -> Option<Value> {
    let is_721 = nft_supports_interface(url, contract, [0x80, 0xac, 0x58, 0xcd]);
    if !is_721 {
        return None; // 1155 needs a known id (manual), not blind enumeration
    }
    let enumerable = nft_supports_interface(url, contract, [0x78, 0x0e, 0x9d, 0x63]);
    if !enumerable {
        return None;
    }
    // balanceOf(addr) (721) = number owned.
    let count = erc20_balance_of(url, contract, addr).unwrap_or(0);
    let count = count.min(200); // bound the eth_call fan-out
    let mut instances = Vec::new();
    for i in 0..count {
        // tokenOfOwnerByIndex(addr, i) = 0x2f745c59
        let a = hex_decode(addr).ok()?;
        let mut data = vec![0x2f, 0x74, 0x5c, 0x59];
        data.extend_from_slice(&left_pad32(&a));
        data.extend_from_slice(&left_pad32(&(i as u128).to_be_bytes()));
        let call = json!([{ "to": contract, "data": format!("0x{}", hex_lower(&data)) }, "latest"]);
        let Ok(ret) = rpc_str(url, "eth_call", call) else { continue };
        let Ok(id_bytes) = hex_decode(&ret) else { continue };
        let id_dec = be32_to_decimal(&id_bytes);
        let (mname, image) = nft_token_uri(url, contract, &id_dec, false)
            .map(|uri| nft_resolve_metadata(&uri))
            .unwrap_or_default();
        let name = if mname.is_empty() { format!("#{id_dec}") } else { mname };
        instances.push(json!({ "id": id_dec, "name": name, "image": image, "amount": "1" }));
    }
    if instances.is_empty() {
        return None;
    }
    Some(json!({ "name": "", "contract": contract, "kind": "721", "instances": instances }))
}

// ── send (money-critical) ────────────────────────────────────────────────

/// Sign and broadcast a native value transfer on the selected EVM chain (symbol +
/// chainId resolved from the registry). `to` is a 0x address, `value_hex` the
/// amount in wei (hex, "0x"-optional). Returns the tx hash.
///
/// Legacy (type-0) EIP-155 tx — small + auditable. On high-baseFee chains (Ethereum)
/// the legacy gasPrice is padded so a rising baseFee can't strand the tx.
pub fn esc_send(phrase: &str, chain: &str, to: &str, value_hex: &str) -> Result<Value, String> {
    esc_send_redeem(phrase, chain, to, value_hex, None)
}

/// As `esc_send`, but redeems `redeem` (with the real fee) INSIDE the signer so a
/// max-fee bound in the grant is enforced against the computed gasPrice*gasLimit.
pub fn esc_send_redeem(phrase: &str, chain: &str, to: &str, value_hex: &str, redeem: Option<SpendRedeem>) -> Result<Value, String> {
    // Money path: require an explicit, known chain — never default-route a transfer (audit).
    if chain.trim().is_empty() {
        return Err("no chain selected for this transfer".into());
    }
    let c = evm_chain(chain)?;
    let sk = signing_key(phrase)?;
    // Reject a typo'd / zero recipient BEFORE signing (audit #4). For a native send
    // the recipient IS the tx `to`.
    let to_bytes = hex_decode(&validate_address(to)?)?;
    let value = be_minimal(&hex_decode(value_hex)?);
    sign_and_send(c, &EvmSigner::Local { sk: &sk },&to_bytes, &value, &[], redeem)
}

/// Native EVM transfer signed by a connected Ledger (Ethereum app). `from` = the 0x
/// address stored for `path` at add-time (drives nonce/gas/funds + the from field).
/// Mirrors esc_send_redeem; the device shows amount + recipient and the user approves
/// on-device. Plain native transfers need NO blind-signing on the Ledger.
pub fn esc_send_ledger(path: &str, from: &str, chain: &str, to: &str, value_hex: &str, redeem: Option<SpendRedeem>) -> Result<Value, String> {
    if chain.trim().is_empty() {
        return Err("no chain selected for this transfer".into());
    }
    let c = evm_chain(chain)?;
    let to_bytes = hex_decode(&validate_address(to)?)?;
    let value = be_minimal(&hex_decode(value_hex)?);
    let from = validate_address(from)?; // normalize the stored Ledger address (EIP-55)
    sign_and_send(c, &EvmSigner::Ledger { path, from: &from }, &to_bytes, &value, &[], redeem)
}

/// ERC-20 token transfer on `chain`: tx `to` = the token CONTRACT (from the trusted
/// registry), value = 0, data = transfer(recipient, amount). The user-entered
/// RECIPIENT is validated (the contract is not — it's ours). `amount_hex` = smallest
/// units (hex). Inherits every safety check via `sign_and_send`. Back-compat shim:
/// the caller redeemed the grant up front (no fee bound).
pub fn evm_token_send(phrase: &str, chain: &str, contract: &str, to: &str, amount_hex: &str) -> Result<Value, String> {
    evm_token_send_redeem(phrase, chain, contract, to, amount_hex, None)
}

/// As `evm_token_send`, but redeems `redeem` (with the real fee) INSIDE the signer
/// so a max-fee bound in the grant is enforced against gasPrice*gasLimit — mirrors
/// `esc_send_redeem`. `None` = the caller already redeemed up front (legacy path).
pub fn evm_token_send_redeem(phrase: &str, chain: &str, contract: &str, to: &str, amount_hex: &str, redeem: Option<SpendRedeem>) -> Result<Value, String> {
    if chain.trim().is_empty() {
        return Err("no chain selected for this transfer".into());
    }
    let c = evm_chain(chain)?;
    let sk = signing_key(phrase)?;
    let recipient = hex_decode(&validate_address(to)?)?; // validate the USER's recipient
    let token = hex_decode(contract)?;
    if token.len() != 20 {
        return Err("bad token contract".into());
    }
    let amount = be_minimal(&hex_decode(amount_hex)?);
    // transfer(address,uint256): selector 0xa9059cbb + left-pad32(recipient) + left-pad32(amount).
    let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
    data.extend_from_slice(&left_pad32(&recipient));
    data.extend_from_slice(&left_pad32(&amount));
    sign_and_send(c, &EvmSigner::Local { sk: &sk },&token, &[], &data, redeem)
}

/// MONEY (irreversible): transfer an ERC-721 NFT on `chain`. tx `to` = the NFT
/// contract; value = 0; data = safeTransferFrom(from,to,tokenId). `from` is
/// DERIVED from the signing key (never UI-passed), so the caller can only move
/// what it owns. `token_id_dec` is the DECIMAL uint256 (the full 256-bit value is
/// preserved via `decimal_to_be32`; it is NEVER routed through u128).
pub fn evm_nft_send_721(phrase: &str, chain: &str, contract: &str, to: &str, token_id_dec: &str) -> Result<Value, String> {
    evm_nft_send_721_redeem(phrase, chain, contract, to, token_id_dec, None)
}

/// As `evm_nft_send_721`, but redeems `redeem` (with the real fee) INSIDE the signer
/// so a max-fee bound in the grant is enforced. `None` = legacy up-front redeem.
pub fn evm_nft_send_721_redeem(phrase: &str, chain: &str, contract: &str, to: &str, token_id_dec: &str, redeem: Option<SpendRedeem>) -> Result<Value, String> {
    if chain.trim().is_empty() {
        return Err("no chain selected for this transfer".into());
    }
    let c = evm_chain(chain)?;
    let sk = signing_key(phrase)?;
    let recipient = hex_decode(&validate_address(to)?)?; // validate the USER's recipient
    let nft = hex_decode(contract)?;
    if nft.len() != 20 {
        return Err("bad NFT contract".into());
    }
    let from_b = address_bytes(&sk); // `from` = US (derived from the key, NOT UI-trusted)
    let tid = decimal_to_be32(token_id_dec)?;
    // safeTransferFrom(address,address,uint256): selector 0x42842e0e
    //   + left-pad32(from) + left-pad32(to) + tokenId(32)
    let mut data = vec![0x42, 0x84, 0x2e, 0x0e];
    data.extend_from_slice(&left_pad32(&from_b));
    data.extend_from_slice(&left_pad32(&recipient));
    data.extend_from_slice(&tid);
    sign_and_send(c, &EvmSigner::Local { sk: &sk },&nft, &[], &data, redeem)
}

/// MONEY (irreversible): transfer `qty` of an ERC-1155 token id on `chain`. tx
/// `to` = the contract; value = 0; data = safeTransferFrom(from,to,id,amount,bytes)
/// with an EMPTY trailing `bytes` (head: from,to,id,amount,offset=0xa0; tail:
/// len=0). `from` is DERIVED from the key. `token_id_dec` + `qty_dec` are decimal.
pub fn evm_nft_send_1155(
    phrase: &str,
    chain: &str,
    contract: &str,
    to: &str,
    token_id_dec: &str,
    qty_dec: &str,
) -> Result<Value, String> {
    evm_nft_send_1155_redeem(phrase, chain, contract, to, token_id_dec, qty_dec, None)
}

/// As `evm_nft_send_1155`, but redeems `redeem` (with the real fee) INSIDE the signer
/// so a max-fee bound in the grant is enforced. `None` = legacy up-front redeem.
pub fn evm_nft_send_1155_redeem(
    phrase: &str,
    chain: &str,
    contract: &str,
    to: &str,
    token_id_dec: &str,
    qty_dec: &str,
    redeem: Option<SpendRedeem>,
) -> Result<Value, String> {
    if chain.trim().is_empty() {
        return Err("no chain selected for this transfer".into());
    }
    let c = evm_chain(chain)?;
    let sk = signing_key(phrase)?;
    let recipient = hex_decode(&validate_address(to)?)?;
    let nft = hex_decode(contract)?;
    if nft.len() != 20 {
        return Err("bad NFT contract".into());
    }
    let from_b = address_bytes(&sk);
    let id = decimal_to_be32(token_id_dec)?;
    let qty = decimal_to_be32(qty_dec)?;
    if qty.iter().all(|b| *b == 0) {
        return Err("quantity must be at least 1".into());
    }
    // safeTransferFrom(address,address,uint256,uint256,bytes): selector 0xf242432a
    // ABI head (5 words): from, to, id, amount, data-offset = 0xa0 (=160 = 5*32).
    // ABI tail: bytes length = 0 (empty `data`).
    let mut data = vec![0xf2, 0x42, 0x43, 0x2a];
    data.extend_from_slice(&left_pad32(&from_b));
    data.extend_from_slice(&left_pad32(&recipient));
    data.extend_from_slice(&id);
    data.extend_from_slice(&qty);
    data.extend_from_slice(&left_pad32(&[0xa0])); // offset to the bytes arg
    data.extend_from_slice(&left_pad32(&[])); // bytes length = 0
    sign_and_send(c, &EvmSigner::Local { sk: &sk },&nft, &[], &data, redeem)
}

/// Shared signer: builds, signs (legacy EIP-155), and broadcasts a tx with all the
/// audited hardening — gasPrice padding + per-chain CEILING, gas estimate+clamp,
/// funds pre-flight, deterministic-hash double-send protection. `to_bytes` = tx
/// recipient/contract, `value` = native wei (minimal, empty=0), `data` = calldata
/// (empty for native). When `redeem` is Some, the spend grant is consumed HERE —
/// AFTER the real fee is known — so a max-fee bound in the grant is enforced
/// against gasPrice*gasLimit before signing (and fails closed before any broadcast).
/// Where the EVM signature comes from. `Local` derives the secp256k1 key from the seed
/// (today's path, byte-for-byte unchanged). `Ledger` asks a connected Ledger (Ethereum
/// app) to sign the SAME EIP-155 preimage over BLE. The seam returns (v, r, s).
enum EvmSigner<'a> {
    Local { sk: &'a SigningKey },
    Ledger { path: &'a str, from: &'a str },
}

impl EvmSigner<'_> {
    /// The EIP-55 from-address (Local derives it from the key; Ledger returns the stored one).
    fn from_addr(&self) -> String {
        match self {
            EvmSigner::Local { sk } => to_checksum(&address_bytes(sk)),
            EvmSigner::Ledger { from, .. } => from.to_string(),
        }
    }

    /// (v, r, s) for `preimage` (the EIP-155 signing rlp).
    ///   Local : keccak256(preimage) → secp256k1 → v = chainId*2+35+recid (UNCHANGED).
    ///   Ledger: hand the device the preimage; the Ethereum app keccak-hashes + signs.
    fn sign(&self, preimage: &[u8], chain_id: u64) -> Result<(u64, [u8; 32], [u8; 32]), String> {
        match self {
            EvmSigner::Local { sk } => {
                let digest = keccak256(preimage);
                let (sig, recid) = sk.sign_prehash_recoverable(&digest).map_err(|e| format!("sign: {e}"))?;
                let rs = sig.to_bytes(); // r(32) || s(32), already low-S normalized
                let mut r = [0u8; 32];
                r.copy_from_slice(&rs[..32]);
                let mut s = [0u8; 32];
                s.copy_from_slice(&rs[32..]);
                Ok((chain_id * 2 + 35 + recid.to_byte() as u64, r, s))
            }
            EvmSigner::Ledger { path, .. } => crate::ledger_evm::sign_legacy(path, preimage, chain_id),
        }
    }
}

fn sign_and_send(c: &EvmChain, signer: &EvmSigner, to_bytes: &[u8], value: &[u8], data: &[u8], redeem: Option<SpendRedeem>) -> Result<Value, String> {
    let from = signer.from_addr();
    let to_param = format!("0x{}", hex_lower(to_bytes));
    let value_param = if value.is_empty() { "0x0".to_string() } else { format!("0x{}", hex_lower(value)) };
    let data_param = format!("0x{}", hex_lower(data));

    // Strict-parse network params — never fabricate a 0 nonce/fee (audit #2, #5).
    let nonce = be_minimal(&hex_decode(&rpc_str(&chain_rpc(c), "eth_getTransactionCount", json!([from, "pending"]))?)?);
    let gas_price_raw = u128_from_hex(&rpc_str(&chain_rpc(c), "eth_gasPrice", json!([]))?)?;
    if gas_price_raw == 0 {
        return Err("network returned a zero gas price — please try again in a moment".into());
    }
    // Clamp the RPC-reported gasPrice to a sane per-chain ceiling FIRST — a lying
    // node can't push the fee arbitrarily high (max-fee/fee-drain hardening).
    let ceiling = gas_price_ceiling(c.chain_id);
    let gas_price_u = gas_price_raw.min(ceiling);
    // Pad the legacy gasPrice so a few blocks of baseFee growth can't strand the tx
    // (Ethereum baseFee is volatile → 2x; ESC + others → +12.5%) (audit: stuck-tx),
    // then re-clamp so the pad can't exceed the ceiling either.
    let gas_price_u = if c.chain_id == 1 { gas_price_u.saturating_mul(2) } else { gas_price_u.saturating_mul(9) / 8 };
    let gas_price_u = gas_price_u.min(ceiling);
    let gas_price = be_minimal(&gas_price_u.to_be_bytes());

    // estimateGas the REAL call (with data) — also pre-flights reverts/insufficient
    // token balance. Same helper as esc_fee_estimate so the confirm dialog's number
    // and the signer's gas limit can't drift (M-1).
    let gas_limit = estimate_gas_limit(c, &from, &to_param, &value_param, &data_param)?;
    let gas = be_u64(gas_limit);

    // The real network fee for this tx (wei). Enforce the grant's max-fee against it
    // and consume the grant HERE — before signing/broadcast — so an inflated fee is
    // rejected, and a single-use grant can't be replayed past a fee mismatch.
    let actual_fee = (gas_limit as u128).saturating_mul(gas_price_u);
    if let Some(r) = redeem {
        crate::guard::redeem_spend_fee(&r.token, &r.kind, &r.to, &r.amount, Some(actual_fee))?;
    }

    // Pre-flight funds check: cover native value + max gas before signing (audit).
    let value_u = u128_from_bytes_sat(value);
    let need = value_u.saturating_add(actual_fee);
    let have = balance_wei(&chain_rpc(c), &from)?;
    if need > have {
        return Err(format!("insufficient funds — need ~{need} wei (amount + gas), balance {have} wei"));
    }

    let chain_id_bytes = be_u64(c.chain_id);
    // EIP-155 signing preimage: rlp([nonce, gasPrice, gas, to, value, data, chainId, 0, 0]).
    let preimage = rlp_list(&[
        rlp_str(&nonce), rlp_str(&gas_price), rlp_str(&gas),
        rlp_str(to_bytes), rlp_str(value), rlp_str(data),
        rlp_str(&chain_id_bytes), rlp_str(&[]), rlp_str(&[]),
    ]);
    // Sign via the seam: Local hashes keccak256(preimage) + secp256k1; Ledger hands the
    // SAME preimage to the Ethereum app, which keccak-hashes + signs and returns (v, r, s).
    let (v, r, s) = signer.sign(&preimage, c.chain_id)?;

    let signed = rlp_list(&[
        rlp_str(&nonce), rlp_str(&gas_price), rlp_str(&gas),
        rlp_str(to_bytes), rlp_str(value), rlp_str(data),
        rlp_str(&be_u64(v)), rlp_str(&be_minimal(&r)), rlp_str(&be_minimal(&s)),
    ]);
    let raw = format!("0x{}", hex_lower(&signed));
    // Deterministic tx hash from the signed bytes → a lost/timed-out broadcast reply
    // can't make us false-report failure and tempt a double-send (audit #1).
    let local_hash = format!("0x{}", hex_lower(&keccak256(&signed)));

    let res = (|| match rpc(&chain_rpc(c), "eth_sendRawTransaction", json!([raw])) {
        Ok(h) => Ok(json!({ "txHash": h.as_str().unwrap_or(&local_hash), "from": from })),
        Err(e) => {
            let el = e.to_lowercase();
            if el.contains("already known") || el.contains("already exists") || el.contains("known transaction") {
                return Ok(json!({ "txHash": local_hash, "from": from, "note": "already broadcast" }));
            }
            if el.contains("timed out") || el.contains("timeout") || el.contains("connection refused")
                || el.contains("dns") || el.contains("io error") || el.contains("transport")
            {
                for _ in 0..6 {
                    if let Ok(found) = rpc(&chain_rpc(c), "eth_getTransactionByHash", json!([local_hash])) {
                        if !found.is_null() {
                            return Ok(json!({ "txHash": local_hash, "from": from, "note": "confirmed after timeout" }));
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
            Err(e)
        }
    })();
    // Money movement is audited AT THE SIGNER — every caller, every code path,
    // success and failure alike (guard.rs: authority must be auditable). No key
    // material enters the record; from/to/value are on-chain-public anyway.
    crate::guard::audit(
        "wallet.send",
        json!({
            "chain": c.key,
            "from": from,
            "to": to_param,
            "value": value_param,
            "data_bytes": data.len(),
            "result": match &res {
                Ok(v) => json!({ "txHash": v.get("txHash") }),
                Err(e) => json!({ "error": e }),
            },
        }),
    );
    res
}

fn left_pad32(b: &[u8]) -> [u8; 32] {
    let mut o = [0u8; 32];
    let n = b.len().min(32);
    o[32 - n..].copy_from_slice(&b[b.len() - n..]);
    o
}

fn u128_from_bytes_sat(b: &[u8]) -> u128 {
    b.iter().fold(0u128, |a, &x| a.saturating_mul(256).saturating_add(x as u128))
}

// ── JSON-RPC over TLS (ureq reuses the rustls backend iroh already builds) ──

fn rpc(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let resp = ureq::post(url)
        .timeout(std::time::Duration::from_secs(20))
        .send_json(body)
        .map_err(|e| format!("rpc {method}: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("rpc {method} decode: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("rpc {method}: {err}"));
    }
    v.get("result").cloned().ok_or_else(|| format!("rpc {method}: no result"))
}

/// Like `rpc`, but require a string result (hex quantity/data). A present-but-
/// non-string result (`null`, a number, an HTML proxy page) is an error — NOT a
/// silently-substituted default, which for a nonce/fee would sign a bad tx.
fn rpc_str(url: &str, method: &str, params: Value) -> Result<String, String> {
    let v = rpc(url, method, params)?;
    v.as_str().map(str::to_string).ok_or_else(|| format!("rpc {method}: expected a string result"))
}

/// HTTP GET → JSON (NFT index + metadata; read-only, no key, no money). Caps the
/// body so a hostile index/gateway can't exhaust memory.
fn http_get_json(url: &str) -> Result<Value, String> {
    // W6-NFT-SSRF: refuse private/loopback/LAN hosts so an attacker-controlled NFT tokenURI
    // can't point this fetch at the on-device BEAM node or a LAN service (deanon / port-probe).
    let host = url_host(url);
    // Two layers: the string parser catches obvious literals/`localhost`, and the resolver
    // catches everything it can't — decimal/hex/octal/short IPv4 literals (which the OS resolver
    // normalizes to 127.0.0.1) and DNS names whose A/AAAA record points at a private host.
    if host.is_empty() || is_private_host(&host) || host_resolves_private(&host) {
        return Err("blocked: URL host is not a public host".into());
    }
    // W6-NFT-SSRF (redirect): do NOT follow redirects — the host guard above only sees the
    // INITIAL url, so a public URL that 302s to http://127.0.0.1 / a LAN host would otherwise
    // reach an internal service. NFT metadata is served directly by the gateway, so redirects
    // aren't needed; a 3xx response then fails JSON parse → blanks (fail-closed).
    let resp = ureq::builder()
        .redirects(0)
        // AUTHORITATIVE SSRF gate: ureq connects only to the addresses this resolver returns, and
        // it strips every private/loopback one — so even an IPv4-mapped-IPv6 literal or a
        // rebinding DNS name (public at check-time, private at connect-time) cannot reach a local
        // service. The up-front string + resolve checks above stay as cheap fail-fast.
        .resolver(public_only_resolver)
        .build()
        .get(url)
        .timeout(std::time::Duration::from_secs(15))
        .set("Accept", "application/json")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let body = resp
        .into_string()
        .map_err(|e| format!("GET {url} read: {e}"))?;
    if body.len() > 4 * 1024 * 1024 {
        return Err("response too large".into());
    }
    serde_json::from_str(&body).map_err(|e| format!("GET {url} decode: {e}"))
}

// ── small, self-contained primitives (no extra crates) ─────────────────────

fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update(data);
    let out = h.finalize();
    let mut r = [0u8; 32];
    r.copy_from_slice(&out);
    r
}

fn hex_val(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        other => Err(format!("bad hex char {:?}", other as char)),
    }
}

fn hex_decode(h: &str) -> Result<Vec<u8>, String> {
    let h = h.trim().trim_start_matches("0x");
    let h = if h.len() % 2 == 1 { format!("0{h}") } else { h.to_string() };
    let b = h.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i < b.len() {
        out.push((hex_val(b[i])? << 4) | hex_val(b[i + 1])?);
        i += 2;
    }
    Ok(out)
}

fn hex_lower(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn u128_from_hex(h: &str) -> Result<u128, String> {
    let h = h.trim().trim_start_matches("0x");
    if h.is_empty() {
        return Ok(0);
    }
    u128::from_str_radix(h, 16).map_err(|e| format!("hex u128: {e}"))
}

fn u64_from_hex(h: &str) -> Result<u64, String> {
    let h = h.trim().trim_start_matches("0x");
    if h.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(h, 16).map_err(|e| format!("hex u64: {e}"))
}

/// Strip leading zero bytes — RLP encodes integers as minimal big-endian.
fn be_minimal(v: &[u8]) -> Vec<u8> {
    let mut i = 0;
    while i < v.len() && v[i] == 0 {
        i += 1;
    }
    v[i..].to_vec()
}

fn be_u64(v: u64) -> Vec<u8> {
    be_minimal(&v.to_be_bytes())
}

/// Decimal uint256 string → 32-byte big-endian (left-padded). uint256 exceeds
/// u128, so this is hand-rolled bignum: accumulate `acc = acc*10 + digit` across
/// a 32-byte buffer. Rejects non-digits and >256-bit overflow. NEVER route a
/// token id through u128 — the calldata MUST carry the exact 256-bit value.
fn decimal_to_be32(dec: &str) -> Result<[u8; 32], String> {
    let s = dec.trim();
    if s.is_empty() {
        return Err("token id is empty".into());
    }
    let mut buf = [0u8; 32];
    for ch in s.bytes() {
        if !ch.is_ascii_digit() {
            return Err("token id must be a decimal number".into());
        }
        let digit = (ch - b'0') as u16;
        // buf = buf*10 + digit, big-endian, with overflow detection.
        let mut carry = digit;
        for byte in buf.iter_mut().rev() {
            let v = (*byte as u16) * 10 + carry;
            *byte = (v & 0xff) as u8;
            carry = v >> 8;
        }
        if carry != 0 {
            return Err("token id exceeds 256 bits".into());
        }
    }
    Ok(buf)
}

/// 32-byte big-endian → decimal string (the inverse of `decimal_to_be32`).
/// Repeated divide-by-10 over the byte buffer. "0" for all-zero input.
fn be32_to_decimal(be: &[u8]) -> String {
    let mut buf: Vec<u8> = be.to_vec();
    // Trim leading zeros for the work buffer (but keep at least one byte).
    let mut digits = Vec::new();
    loop {
        // divide buf by 10, collecting the remainder as the next least-significant digit.
        let mut rem = 0u16;
        let mut all_zero = true;
        for byte in buf.iter_mut() {
            let cur = (rem << 8) | (*byte as u16);
            *byte = (cur / 10) as u8;
            rem = cur % 10;
            if *byte != 0 {
                all_zero = false;
            }
        }
        digits.push(b'0' + rem as u8);
        if all_zero {
            break;
        }
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_else(|_| "0".into())
}

fn format_token(wei: u128) -> String {
    let whole = wei / WEI_PER_TOKEN;
    let frac6 = (wei % WEI_PER_TOKEN) / 1_000_000_000_000; // 6 decimal places
    if frac6 == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac6:06}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ── minimal RLP encoder (hand-rolled so the tx bytes are fully auditable) ───

fn rlp_len_prefix(len: usize, offset: u8) -> Vec<u8> {
    if len <= 55 {
        vec![offset + len as u8]
    } else {
        let lb = be_minimal(&(len as u64).to_be_bytes());
        let mut out = vec![offset + 55 + lb.len() as u8];
        out.extend_from_slice(&lb);
        out
    }
}

fn rlp_str(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    let mut out = rlp_len_prefix(bytes.len(), 0x80);
    out.extend_from_slice(bytes);
    out
}

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = Vec::new();
    for it in items {
        payload.extend_from_slice(it);
    }
    let mut out = rlp_len_prefix(payload.len(), 0xc0);
    out.extend_from_slice(&payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known BIP39 test vector → standard m/44'/60'/0'/0/0 Ethereum address.
    // "test test test test test test test test test test test junk" is the
    // canonical Hardhat/Foundry mnemonic; account 0 = 0xf39F...2266.
    #[test]
    fn eth_parity_vector() {
        let phrase = "test test test test test test test test test test test junk";
        let addr = esc_address(phrase).unwrap();
        assert_eq!(addr, "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
    }

    #[test]
    fn signing_url_https_ok_public_http_rejected_private_http_insecure() {
        // https → always accepted, not flagged insecure.
        assert_eq!(classify_signing_url("https://api.elastos.io/esc"), Ok(false));
        // public cleartext → rejected (the H6 money-path harm).
        assert!(classify_signing_url("http://api.elastos.io/esc").is_err());
        assert!(classify_signing_url("http://8.8.8.8:8545").is_err());
        assert!(classify_signing_url("http://evil.example.com").is_err());
        // loopback / RFC1918 cleartext → accepted but flagged insecure.
        assert_eq!(classify_signing_url("http://127.0.0.1:8545"), Ok(true));
        assert_eq!(classify_signing_url("http://localhost:8545"), Ok(true));
        assert_eq!(classify_signing_url("http://10.0.0.5:8545"), Ok(true));
        assert_eq!(classify_signing_url("http://192.168.1.10:8545"), Ok(true));
        assert_eq!(classify_signing_url("http://172.16.0.1:8545"), Ok(true));
        assert_eq!(classify_signing_url("http://172.31.255.254:8545"), Ok(true));
        assert_eq!(classify_signing_url("http://[::1]:8545"), Ok(true));
        // 172.32 is OUTSIDE the /12 → public → rejected.
        assert!(classify_signing_url("http://172.32.0.1:8545").is_err());
        // userinfo can't smuggle a private host past a public authority.
        assert!(classify_signing_url("http://127.0.0.1@evil.com/").is_err());
        // neither scheme → rejected.
        assert!(classify_signing_url("ftp://127.0.0.1").is_err());
    }

    #[test]
    fn private_host_classification() {
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("10.255.255.255"));
        assert!(is_private_host("192.168.0.1"));
        assert!(is_private_host("172.20.0.1"));
        assert!(is_private_host("localhost"));
        assert!(is_private_host("::1"));
        assert!(!is_private_host("8.8.8.8"));
        assert!(!is_private_host("172.15.0.1"));
        assert!(!is_private_host("172.32.0.1"));
        assert!(!is_private_host("example.com"));
    }

    #[test]
    fn resolve_based_private_check() {
        // Dotted-quad literals resolve deterministically offline (parsed as a SocketAddr
        // literal, no DNS): the resolver layer must agree with the string parser on these.
        assert!(host_resolves_private("127.0.0.1"));
        assert!(host_resolves_private("10.0.0.1"));
        assert!(host_resolves_private("192.168.1.1"));
        assert!(!host_resolves_private("8.8.8.8"));
        assert!(!host_resolves_private("1.1.1.1"));
        // Unresolvable host → fail-closed (treated as private/blocked).
        assert!(host_resolves_private("nonexistent.invalid"));
        // ip_is_private spot checks across families.
        assert!(ip_is_private("127.0.0.1".parse().unwrap()));
        assert!(ip_is_private("169.254.1.1".parse().unwrap())); // link-local
        assert!(ip_is_private("0.0.0.0".parse().unwrap())); // unspecified
        assert!(ip_is_private("::1".parse().unwrap()));
        assert!(ip_is_private("fe80::1".parse().unwrap())); // v6 link-local
        assert!(ip_is_private("fc00::1".parse().unwrap())); // v6 unique-local
        assert!(!ip_is_private("8.8.8.8".parse().unwrap()));
        assert!(!ip_is_private("2606:4700::1111".parse().unwrap()));
        // Verifier-found bypasses that MUST now be blocked:
        assert!(ip_is_private("::ffff:127.0.0.1".parse().unwrap())); // IPv4-mapped loopback
        assert!(ip_is_private("::ffff:7f00:1".parse().unwrap())); // same, hextet form
        assert!(ip_is_private("::ffff:10.0.0.5".parse().unwrap())); // mapped private
        assert!(ip_is_private("::ffff:192.168.1.1".parse().unwrap())); // mapped private
        assert!(ip_is_private("::ffff:169.254.1.1".parse().unwrap())); // mapped link-local
        assert!(ip_is_private("64:ff9b::7f00:1".parse().unwrap())); // NAT64-embedded loopback
        assert!(ip_is_private("100.64.0.1".parse().unwrap())); // CGNAT 100.64/10
        // A genuine public address mapped into v6 must STILL be allowed (no over-block):
        assert!(!ip_is_private("::ffff:8.8.8.8".parse().unwrap()));
        assert!(!ip_is_private("64:ff9b::808:808".parse().unwrap())); // NAT64-embedded 8.8.8.8
        // Deprecated IPv4-compatible ::a.b.c.d hygiene:
        assert!(ip_is_private("::127.0.0.1".parse().unwrap())); // ::a.b.c.d loopback
        assert!(ip_is_private("::10.0.0.1".parse().unwrap())); // ::a.b.c.d private
        assert!(!ip_is_private("::8.8.8.8".parse().unwrap())); // ::a.b.c.d public stays public
    }

    #[test]
    fn rlp_basics() {
        assert_eq!(rlp_str(&[]), vec![0x80]);
        assert_eq!(rlp_str(&[0x7f]), vec![0x7f]);
        assert_eq!(rlp_str(&[0x80]), vec![0x81, 0x80]);
        assert_eq!(rlp_list(&[rlp_str(&[]), rlp_str(&[])]), vec![0xc2, 0x80, 0x80]);
    }

    #[test]
    fn checksum_known() {
        // EIP-55 reference address.
        let bytes = hex_decode("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        let mut a = [0u8; 20];
        a.copy_from_slice(&bytes);
        assert_eq!(to_checksum(&a), "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }

    // ── NFT calldata golden vectors (gate the irreversible transfer path) ──────

    /// Build the exact safeTransferFrom(address,address,uint256) 721 calldata the
    /// signer would broadcast, independent of any network — so we can assert it
    /// byte-for-byte. (Mirrors the body of `evm_nft_send_721` minus the send.)
    fn calldata_721(from: &[u8; 20], to: &[u8; 20], token_id_dec: &str) -> Vec<u8> {
        let tid = decimal_to_be32(token_id_dec).unwrap();
        let mut data = vec![0x42, 0x84, 0x2e, 0x0e];
        data.extend_from_slice(&left_pad32(from));
        data.extend_from_slice(&left_pad32(to));
        data.extend_from_slice(&tid);
        data
    }

    fn calldata_1155(from: &[u8; 20], to: &[u8; 20], id_dec: &str, qty_dec: &str) -> Vec<u8> {
        let id = decimal_to_be32(id_dec).unwrap();
        let qty = decimal_to_be32(qty_dec).unwrap();
        let mut data = vec![0xf2, 0x42, 0x43, 0x2a];
        data.extend_from_slice(&left_pad32(from));
        data.extend_from_slice(&left_pad32(to));
        data.extend_from_slice(&id);
        data.extend_from_slice(&qty);
        data.extend_from_slice(&left_pad32(&[0xa0]));
        data.extend_from_slice(&left_pad32(&[]));
        data
    }

    #[test]
    fn nft_calldata_721_golden() {
        let from = {
            let mut a = [0u8; 20];
            a.copy_from_slice(&hex_decode("1111111111111111111111111111111111111111").unwrap());
            a
        };
        let to = {
            let mut a = [0u8; 20];
            a.copy_from_slice(&hex_decode("2222222222222222222222222222222222222222").unwrap());
            a
        };
        // token id 5
        let data = calldata_721(&from, &to, "5");
        let expect = concat!(
            "42842e0e",
            "0000000000000000000000001111111111111111111111111111111111111111",
            "0000000000000000000000002222222222222222222222222222222222222222",
            "0000000000000000000000000000000000000000000000000000000000000005",
        );
        assert_eq!(hex_lower(&data), expect);
        assert_eq!(data.len(), 4 + 32 * 3); // selector + 3 words
    }

    #[test]
    fn nft_calldata_1155_golden_trailing_offset_and_empty_bytes() {
        let from = {
            let mut a = [0u8; 20];
            a.copy_from_slice(&hex_decode("1111111111111111111111111111111111111111").unwrap());
            a
        };
        let to = {
            let mut a = [0u8; 20];
            a.copy_from_slice(&hex_decode("2222222222222222222222222222222222222222").unwrap());
            a
        };
        // id 5, qty 3 — the qty MUST be its own word (binding "send #5" to exactly 3).
        let data = calldata_1155(&from, &to, "5", "3");
        let expect = concat!(
            "f242432a",
            "0000000000000000000000001111111111111111111111111111111111111111", // from
            "0000000000000000000000002222222222222222222222222222222222222222", // to
            "0000000000000000000000000000000000000000000000000000000000000005", // id
            "0000000000000000000000000000000000000000000000000000000000000003", // amount
            "00000000000000000000000000000000000000000000000000000000000000a0", // bytes offset = 0xa0
            "0000000000000000000000000000000000000000000000000000000000000000", // bytes length = 0
        );
        assert_eq!(hex_lower(&data), expect);
        assert_eq!(data.len(), 4 + 32 * 6); // selector + 6 words (5 head + 1 tail len)
    }

    #[test]
    fn nft_uint256_decimal_roundtrip_above_u128() {
        // A token id larger than u128::MAX must survive verbatim (decimal→be32→decimal).
        let big = "115792089237316195423570985008687907853269984665640564039457584007913129639935"; // 2^256 - 1
        let be = decimal_to_be32(big).unwrap();
        assert_eq!(be, [0xffu8; 32]);
        assert_eq!(be32_to_decimal(&be), big);
        // Round-trip a mid-range value too.
        let mid = "340282366920938463463374607431768211456"; // 2^128 (> u128::MAX)
        assert_eq!(be32_to_decimal(&decimal_to_be32(mid).unwrap()), mid);
        assert_eq!(be32_to_decimal(&decimal_to_be32("0").unwrap()), "0");
        // Overflow + bad input rejected (not panicked).
        let too_big = "115792089237316195423570985008687907853269984665640564039457584007913129639936"; // 2^256
        assert!(decimal_to_be32(too_big).is_err());
        assert!(decimal_to_be32("12x3").is_err());
    }

    #[test]
    fn nft_abi_string_decode_and_clamp() {
        // offset=0x20, len=5, "hello" + right padding.
        let ok = concat!(
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000005",
            "68656c6c6f000000000000000000000000000000000000000000000000000000",
        );
        assert_eq!(decode_abi_string(ok).unwrap(), "hello");
        // A declared length far beyond the buffer is rejected (no OOM/panic).
        let evil = concat!(
            "0000000000000000000000000000000000000000000000000000000000000020",
            "00000000000000000000000000000000000000000000000000000000ffffffff", // ~4GB
            "00000000000000000000000000000000000000000000000000000000000000ff",
        );
        assert!(decode_abi_string(evil).is_err());
    }

    #[test]
    fn address_validation() {
        // canonical checksummed → accepted, returned verbatim
        assert_eq!(
            validate_address("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap(),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
        // all-lowercase → accepted, returned checksummed
        assert_eq!(
            validate_address("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").unwrap(),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
        // mixed-case with a WRONG checksum (typo: trailing d→e) → rejected
        assert!(validate_address("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAee").is_err());
        // zero / burn address → rejected
        assert!(validate_address("0x0000000000000000000000000000000000000000").is_err());
        // bad length / missing prefix → rejected
        assert!(validate_address("0x1234").is_err());
        assert!(validate_address("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").is_err());
    }
}
