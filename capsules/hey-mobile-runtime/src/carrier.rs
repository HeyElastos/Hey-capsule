//! In-process carrier — the `elastos://peer/*` transport, embedded.
//!
//! This is the on-device equivalent of the runtime's built-in Carrier
//! (`elastos-server/src/carrier.rs`). It runs an iroh-1.0 + iroh-gossip node
//! INSIDE the app process and answers the op set hey-core actually calls
//! (`connect`, `gossip_join` direct, `gossip_join_peers`, `gossip_send`,
//! `gossip_recv`, `list_topic_peers`, `get_ticket`, `peer_paths`, `list_peers`)
//! — NOT the standalone peer-provider's stdio op set. The iroh layer (endpoint
//! builder, MemoryLookup, Gossip/Router) is lifted from `peer-provider/src/main.rs`,
//! which is already on the exact iroh 1.0-rc.1 / gossip 0.100 pins.
//!
//! Durability mirrors peer-provider: subscriptions + the per-topic message log
//! + per-consumer cursors are persisted, so a backgrounded/restarted app still
//! receives invites and never drops in-flight messages.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Count of inbound NON-feed broker messages (DM/group). Lets the LOCKED app
/// post a generic "new message" notification with no DEK — content stays
/// sealed in the buffer until biometric unlock.
static INBOUND_COUNT: AtomicU64 = AtomicU64::new(0);
/// De-dups concurrent network_changed() (unlock + IP-change can fire together)
/// so we don't re-dial every topic several times in a burst.
static IN_NET_CHANGE: AtomicBool = AtomicBool::new(false);
pub fn inbound_count() -> u64 {
    INBOUND_COUNT.load(Ordering::Relaxed)
}
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::engine::general_purpose::STANDARD as B64_STD;   // blobs data_base64 / bytes = STD (matches hey-core)
use base64::Engine as _;
use bytes::Bytes;
use n0_future::StreamExt;
use serde::{Deserialize, Serialize};
use std::future::IntoFuture;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use serde_json::{json, Value};

use iroh::endpoint::presets;
use iroh::{protocol::Router, Endpoint, EndpointAddr, EndpointId, RelayConfig, RelayMap, RelayMode, SecretKey};

/// Optional custom relays from the ELASTOS_RELAY_URL env (comma/space
/// separated), e.g. a dev pointing a build at a test relay. The DEFAULT relay
/// set is iroh's own production relays, taken at bind time from
/// `RelayMode::Default.relay_map()` — we do NOT hardcode any relay hostnames
/// here (n0's or Hey's): rc.1's prod relays already moved once
/// (`*.relay.n0.iroh-canary.iroh.link.`, not the old `*.relay.iroh.network.`),
/// so hardcoded literals just rot into dead entries. Customs only LAYER on top
/// of that default; they never replace it.
/// The Hey federation relay — version-matched to the client's iroh 1.0-rc.1.
/// It is part of the DEFAULT relay map (alongside iroh's n0 relays) so two Hey
/// phones ALWAYS share one known, rc.1-matched meeting point. Dropping it (and
/// relying on n0's relays alone) was the transport regression: n0's public
/// relays carry no version guarantee, and two phones latency-probing the n0 map
/// can home on DIFFERENT relays and never mesh — so DMs/invites silently failed.
/// A user's own relay-url.txt still takes priority; this is purely additive.
const FEDERATION_RELAY: &str = "https://elastos.app";  // :443 (the deployed iroh-relay rc.1)

fn env_relays() -> Vec<String> {
    std::env::var("ELASTOS_RELAY_URL")
        .map(|env| {
            env.split(|c: char| c == ',' || c.is_whitespace())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Best-effort discovery of our primary outbound IP WITHOUT interface
/// enumeration (Android/SELinux blocks the netlink dump iroh relies on): bind a
/// UDP socket and "connect" it to a public address — the OS picks the source
/// address for that route and we read it back. No packet is sent; no permission
/// needed. For IPv6 this is typically a globally-routable address (direct P2P
/// works); for IPv4 it's usually the LAN/CGNAT address (useful on the same LAN).
fn primary_ip(v6: bool) -> Option<std::net::IpAddr> {
    // Try several well-known public resolvers: behind a VPN one target may be unroutable while
    // another works, so a single probe (the old behavior) wrongly concluded "no IPv6" and the UI
    // mislabeled an IPv6-only device as IPv4. First reachable target wins.
    let (bind, targets): (&str, &[&str]) = if v6 {
        (
            "[::]:0",
            &[
                "[2001:4860:4860::8888]:53", // Google
                "[2606:4700:4700::1111]:53", // Cloudflare
                "[2620:fe::fe]:53",          // Quad9
            ],
        )
    } else {
        ("0.0.0.0:0", &["8.8.8.8:53", "1.1.1.1:53"])
    };
    for target in targets {
        let Ok(sock) = std::net::UdpSocket::bind(bind) else { continue };
        if sock.connect(target).is_err() {
            continue;
        }
        let Ok(local) = sock.local_addr() else { continue };
        let ip = local.ip();
        let link_local_v6 = matches!(ip, std::net::IpAddr::V6(a) if (a.segments()[0] & 0xffc0) == 0xfe80);
        if ip.is_loopback() || ip.is_unspecified() || link_local_v6 {
            continue;
        }
        return Some(ip);
    }
    None
}

/// True if `ip` is globally routable (so a peer on another network can reach it
/// directly). Excludes RFC1918/CGNAT IPv4 and ULA/link-local IPv6 — those only
/// work on the same LAN, which shouldn't claim full relay-free direct.
fn is_global_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(a) => {
            let o = a.octets();
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_unspecified()
                || a.is_broadcast()
                || a.is_documentation()
                || (o[0] == 100 && (o[1] & 0xc0) == 0x40)        // 100.64/10 CGNAT
                || (o[0] == 192 && o[1] == 0 && o[2] == 0))      // 192.0.0.0/24 — 464XLAT CLAT / IETF, not routable
        }
        std::net::IpAddr::V6(a) => {
            let s = a.segments()[0];
            (s & 0xe000) == 0x2000 // 2000::/3 global unicast (excludes fc00::/7 ULA, fe80::/10)
        }
    }
}
use iroh_gossip::{
    api::{Event, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
// Direct-P2P content (cross-device media). Same crate/version (0.102) + same wire
// contract as the desktop blobs-provider, so a mobile attachment ticket is fetchable
// by a VPS box and vice-versa — it just shares THIS endpoint instead of its own.
use iroh_blobs::{store::fs::FsStore, ticket::BlobTicket, BlobsProtocol};

// ── Durable broker (per-topic log + per-consumer cursor) ─────────────────────

#[derive(Clone, Serialize, Deserialize)]
struct Msg {
    seq: u64,
    content: String,
    sender_id: String,
    ts: i64,
    signature: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Topic {
    log: Vec<Msg>,
    cursors: HashMap<String, usize>,
}

#[derive(Default, Serialize, Deserialize)]
struct Broker {
    topics: BTreeMap<String, Topic>,
    next_seq: u64,
    /// topic -> bootstrap tickets, re-joined on boot so we listen before the app opens.
    #[serde(default)]
    subscriptions: BTreeMap<String, Vec<String>>,
}

impl Broker {
    fn t(&mut self, name: &str) -> &mut Topic {
        self.topics.entry(name.to_string()).or_default()
    }
    fn append(&mut self, topic: &str, content: String, sender_id: String, ts: i64, signature: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        // Bound the per-topic log so a stream of (large) media chunks can't grow
        // broker.json without limit. Drop the oldest entries past the cap and
        // shift the index-based consumer cursors to match.
        const LOG_CAP: usize = 512;
        let t = self.t(topic);
        t.log.push(Msg { seq, content, sender_id, ts, signature });
        if t.log.len() > LOG_CAP {
            let drop = t.log.len() - LOG_CAP;
            t.log.drain(0..drop);
            for c in t.cursors.values_mut() {
                *c = c.saturating_sub(drop);
            }
        }
        seq
    }
    fn drain(&mut self, topic: &str, limit: usize, consumer: &str, skip: Option<&str>) -> Vec<Value> {
        let t = self.t(topic);
        let mut idx = *t.cursors.get(consumer).unwrap_or(&0);
        let mut out = Vec::new();
        while idx < t.log.len() && out.len() < limit {
            let m = &t.log[idx];
            idx += 1;
            if let Some(s) = skip {
                if m.sender_id == s {
                    continue;
                }
            }
            out.push(json!({
                "content": m.content, "message": m.content,
                "sender_id": m.sender_id, "ts": m.ts, "signature": m.signature, "seq": m.seq
            }));
        }
        t.cursors.insert(consumer.to_string(), idx);
        out
    }
}

// ── Carrier ──────────────────────────────────────────────────────────────────

pub struct Carrier {
    endpoint: Endpoint,
    gossip: Gossip,
    _router: Router,
    /// iroh-blobs persistent store — backs the `blobs/*` provider (cross-device media).
    store: FsStore,
    mem: iroh::address_lookup::MemoryLookup,
    senders: Mutex<HashMap<String, GossipSender>>,
    /// topic -> live gossip neighbors (node-id strings). The delivery signal
    /// hey-core's has_topic_peer() polls: an empty set means broadcast is a
    /// silent no-op, so we must report it truthfully.
    neighbors: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    broker: Arc<Mutex<Broker>>,
    dir: PathBuf,
    /// True once iroh has reached a relay / learned its address (endpoint.online()).
    online: Arc<AtomicBool>,
    /// True when iroh sees a usable UDP path (udp_v4 || udp_v6) → direct P2P is
    /// possible and the relay only introduces peers. False = relay carries the
    /// (still E2E-encrypted) data, e.g. hard-NAT cellular.
    direct: Arc<AtomicBool>,
    /// The net_report direct-path flags, surfaced individually in the connection
    /// UI as the proof the agnostic discovery chain works (the patched netdev
    /// feeds iroh its interfaces via getifaddrs on Android, so these can be true).
    udp_v4: Arc<AtomicBool>,
    udp_v6: Arc<AtomicBool>,
    /// True when the direct-P2P bridge advertised a globally-routable address
    /// (global IPv6, a public reflexive addr, or a WG overlay) — a hint that
    /// peers can reach us directly.
    direct_global: Arc<AtomicBool>,
    /// EVERY peer ticket we've ever decoded (contacts, follows, group members),
    /// persisted to `known-peers.json`. After BOTH ends restart (e.g. an app
    /// update) a topic's original bootstrap can be too thin to re-find the peer,
    /// so the self-heal re-seeds every empty topic with this whole set — the peer
    /// keeps the SAME endpoint id across restarts, so the relay re-resolves it.
    known: Arc<Mutex<std::collections::BTreeSet<String>>>,
    /// NAT-observed PUBLIC reflexive addresses from iroh net_report (what the relay sees us as) —
    /// the device's real public IPv4 / IPv6, surfaced in the connection UI. std Mutex so the sync
    /// `net_addrs()` reader doesn't need the runtime.
    observed_v4: Arc<std::sync::Mutex<Option<String>>>,
    observed_v6: Arc<std::sync::Mutex<Option<String>>>,
    /// The LOCAL addresses the carrier advertises (interface + reflexive) — shown
    /// in the connection UI so the user can SEE which interface Hey binds (WiFi
    /// 192.168.x vs a VPN tun 10.x) and verify a split-tunnel.
    advertised: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Carrier {
    pub async fn start(dir: PathBuf, sk: SecretKey) -> anyhow::Result<Arc<Carrier>> {
        tokio::fs::create_dir_all(&dir).await.ok();
        // The node key is derived from the identity seed (passed in), so we never
        // persist a plaintext carrier-secret.key. Remove any legacy one.
        let _ = tokio::fs::remove_file(dir.join("carrier-secret.key")).await;
        let mem = iroh::address_lookup::MemoryLookup::new();
        // DNS — DoH over :443 FIRST, UDP :53 as fast-path fallback.
        //
        // Two Android facts force this (confirmed against iroh-dns source):
        //  1. the DEFAULT resolver reads system DNS via JNI (ndk_context) and
        //     PANICS in release if the JVM context isn't installed → bind dies;
        //  2. mobile/wifi networks routinely block outbound UDP/53 to public
        //     resolvers, so a UDP-only resolver can't resolve the relay
        //     hostnames → the HTTPS relay probe fails → endpoint.online() never
        //     resolves → "carrier starting…" forever.
        // DoH rides 443 (always open where the relays themselves are reachable).
        // 1.1.1.1 / 8.8.8.8 carry their own IPs as cert SANs, so the IP-literal
        // server name validates with no extra TLS config.
        // Include IPv6 resolvers too: on an IPv6-only / IPv6-preferred network (e.g. reaching an
        // IPv6-only VPS/relay), the IPv4-literal resolvers below are unreachable, so DNS — and thus
        // relay discovery — would fail. The v6 DoH endpoints keep resolution working there.
        let dns = iroh::dns::DnsResolver::builder()
            .with_nameservers([
                ("1.1.1.1:443".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Https),
                ("8.8.8.8:443".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Https),
                ("[2606:4700:4700::1111]:443".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Https),
                ("[2001:4860:4860::8888]:443".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Https),
                ("1.1.1.1:53".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Udp),
                ("8.8.8.8:53".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Udp),
                ("[2606:4700:4700::1111]:53".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Udp),
                ("[2001:4860:4860::8888]:53".parse::<std::net::SocketAddr>().unwrap(), iroh::dns::DnsProtocol::Udp),
            ])
            .build();

        // Relay map: iroh's own production relays are ALWAYS in the map — they
        // are THE default, taken live from `RelayMode::Default` so we never ship
        // stale hostnames. Optional customs go in front of them:
        //   <dir>/relay-url.txt   the in-app "my Hey relay" setting (Profile →
        //                         Connection). Only an actual URL counts; blank
        //                         or any legacy keyword falls back to default.
        //   ELASTOS_RELAY_URL     extra relays (comma/space-separated), e.g. a
        //                         dev pointing at a test relay.
        // Order does NOT pick the home relay — iroh latency-probes the whole map
        // (net_report) and homes on the fastest reachable one — so "custom first"
        // merely guarantees the custom is present. Peers on other relays stay
        // reachable either way: iroh dials whatever home relay a peer's address
        // advertises, in-map or not — the map only selects OUR home relay (and
        // which relays QAD probes).
        let saved = std::fs::read_to_string(dir.join("relay-url.txt"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let mut custom: Vec<String> = Vec::new();
        if !saved.is_empty() && saved.parse::<url::Url>().is_ok() {
            custom.push(saved);
        }
        custom.extend(env_relays());
        let mut configs: Vec<std::sync::Arc<RelayConfig>> = custom
            .iter()
            .filter_map(|s| s.parse::<url::Url>().ok())
            .map(|url| std::sync::Arc::new(RelayConfig::new(url.into(), Some(Default::default()))))
            .collect();
        // The map carries EVERY relay option so two phones always share a WORKING
        // broker: any custom relay, the elastos.app federation relay, AND n0's
        // own rc.1 relays. n0's relays are version-matched to this client, so
        // even if elastos.app is down or can't broker (both phones reached it but
        // couldn't connect = a relay that accepts but won't forward), n0 catches
        // it. iroh latency-homes on the fastest reachable one; peers reach each
        // other via whichever broker both can use.
        // DEFAULT relay set = n0's rc.1 relays only. We do NOT hardcode a Hey
        // federation relay: elastos.app bare is a WEBSITE (:443) and the legacy
        // :8443 relay is unreliable, so a hardcoded entry would just be a dead or
        // wrong endpoint. A user-deployed relay goes in via relay-url.txt (their
        // own URL FIRST), with n0 always appended as the version-matched fallback.
        // DEFAULT (no custom set) = the Elastos.app COMMUNITY federation relay
        // (a confirmed rc.1-matched relay), with n0's relays as fallback. A custom
        // relay (relay-url.txt) REPLACES elastos.app; n0 always stays as backup.
        if custom.is_empty() {
            if let Ok(url) = FEDERATION_RELAY.parse::<url::Url>() {
                configs.push(std::sync::Arc::new(RelayConfig::new(url.into(), Some(Default::default()))));
            }
        }
        // n0's public relays ride iroh's release schedule (currently rc/canary, NOT
        // a tagged 1.0). A client that latency-homes on an n0 relay can hit a version
        // skew vs the rc.1-matched elastos.app relay and fail QUIC multipath
        // negotiation (MultipathNotNegotiated → no gossip neighbor). So allow pinning
        // to the version-matched relay ONLY — `ELASTOS_RELAY_ONLY=1` or a
        // `relay-standard-off` marker in the data dir — which drops the n0 fallback
        // until iroh ships a tagged 1.0. Opt-in: the phone build is unaffected unless
        // it sets it.
        let relay_only = std::env::var("ELASTOS_RELAY_ONLY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            || dir.join("relay-standard-off").exists();
        if !relay_only {
            configs.extend(RelayMode::Default.relay_map().relays::<Vec<std::sync::Arc<RelayConfig>>>());
        }
        let n0 = if relay_only { "n0 OFF (iroh-standard disabled until 1.0)" } else { "n0 fallback" };
        if custom.is_empty() {
            log::info!("carrier relays: elastos.app community + {n0}");
        } else {
            log::info!("carrier relays: custom=[{}] + {n0}", custom.join(", "));
        }

        // N0 preset = pkarr/DNS discovery + QUIC + hole-punching. Keep that, but
        // override the relay map and DNS resolver per the above. PLUS mDNS LAN
        // discovery so two devices on the SAME network find each other's LOCAL
        // address and connect DIRECTLY — no relay, no NAT, no hole-punch needed.
        // (Android also needs a WifiManager.MulticastLock to RECEIVE the mDNS
        // multicast — acquired on the Kotlin side; without it we still send but
        // hear nothing.) mDNS is best-effort: if it can't start, the carrier still
        // comes up on the global/relay path.
        let node_id = sk.public();
        let builder = Endpoint::builder(presets::N0)
            .secret_key(sk)
            .dns_resolver(dns)
            .address_lookup(mem.clone())
            .relay_mode(RelayMode::Custom(RelayMap::from_iter(configs)));
        let builder = match iroh_mdns_address_lookup::MdnsAddressLookup::builder().build(node_id) {
            Ok(mdns) => {
                log::info!("carrier: mDNS LAN discovery ON (same-network peers connect direct)");
                builder.address_lookup(mdns)
            }
            Err(e) => {
                log::warn!("carrier: mDNS init failed ({e}) — LAN-direct off; global/relay still work");
                builder
            }
        };
        let endpoint = builder.bind().await?;
        // Raise the gossip frame cap from the 4 KB default so we can ship media
        // (photo bytes) over the carrier in a few chunks. Both peers run this
        // carrier, so they agree on the limit.
        let gossip = Gossip::builder()
            .max_message_size(1024 * 1024)
            .spawn(endpoint.clone());
        // iroh-blobs store + protocol on the SAME endpoint. ONE endpoint serves both
        // ALPNs; the Router dispatches each inbound connection by its negotiated ALPN
        // (gossip = messaging, blobs = large-file pull). Mirrors the desktop provider.
        let store = FsStore::load(dir.join("blobs")).await?;
        let blobs = BlobsProtocol::new(&store, None);
        let router = Router::builder(endpoint.clone())
            .accept(GOSSIP_ALPN, gossip.clone())
            .accept(iroh_blobs::ALPN, blobs)
            // 1:1 voice calls: μ-law audio over QUIC datagrams on the SAME endpoint. The callee's
            // Router dispatches an inbound voice connection to this handler (the caller dials).
            .accept(crate::voice::VOICE_ALPN, crate::voice::VoiceProtocol)
            // 1:1 video calls (direct-only): H.264 frames over QUIC uni-streams on
            // the SAME endpoint. Same per-call roster auth as voice; the UI gates
            // the offer to a direct path.
            .accept(crate::video::VIDEO_ALPN, crate::video::VideoProtocol)
            // Verse REALTIME lane: movement at game rate over QUIC datagrams on
            // the same endpoint — E2E encrypted by the connection itself, no
            // per-packet ratchet, no disk writes (the DM-lane lag fix).
            .accept(crate::verse_rt::VERSE_RT_ALPN, crate::verse_rt::VerseRtProtocol)
            .spawn();
        // Online detection + observability. Drive `online` off the home-relay
        // connection state (the load-bearing signal) rather than online()'s
        // exact predicate, and log net_report every tick so a stuck carrier is
        // diagnosable from logcat (preferred_relay=None => DNS/probe failure;
        // udp_v4/v6=false on a punchable network => a discovery gap; the patched
        // netdev getifaddrs path is what keeps it TRUE on Android).
        let online = Arc::new(AtomicBool::new(false));
        let direct = Arc::new(AtomicBool::new(false));
        let udp_v4 = Arc::new(AtomicBool::new(false));
        let udp_v6 = Arc::new(AtomicBool::new(false));
        let advertised = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let observed_v4 = Arc::new(std::sync::Mutex::new(None::<String>));
        let observed_v6 = Arc::new(std::sync::Mutex::new(None::<String>));
        // The relay-observed PUBLIC reflexive transport addrs (ip:PORT) — learned
        // from the NETWORK (the relay sees our public mapping), not from Android's
        // blocked interface list. We ADVERTISE these to iroh so a remote peer gets
        // a hole-punchable candidate even though Android suppresses iroh's own QAD.
        // This is what flips a cone-NAT phone (home WiFi, public-IPv4 router) onto
        // a DIRECT path; symmetric CGNAT still can't punch (the port isn't reused).
        let reflex_v4 = Arc::new(std::sync::Mutex::new(None::<std::net::SocketAddr>));
        let reflex_v6 = Arc::new(std::sync::Mutex::new(None::<std::net::SocketAddr>));
        {
            let ep = endpoint.clone();
            let online = online.clone();
            let direct = direct.clone();
            let udp_v4 = udp_v4.clone();
            let udp_v6 = udp_v6.clone();
            let observed_v4 = observed_v4.clone();
            let observed_v6 = observed_v6.clone();
            let reflex_v4 = reflex_v4.clone();
            let reflex_v6 = reflex_v6.clone();
            tokio::spawn(async move {
                use iroh::Watcher as _;
                let mut hrw = ep.home_relay_status();
                let mut nrw = ep.net_report();
                loop {
                    // Truthful online flag: track the CURRENT relay state, both ways.
                    // (It used to latch true forever, so a relay outage looked healthy
                    // and the UI could never report the reconnect that followed.)
                    let mut any_connected = false;
                    for s in &hrw.get() {
                        if let Some(e) = s.last_error() {
                            log::warn!("carrier relay {} error: {e}", s.url());
                        }
                        if s.is_connected() {
                            any_connected = true;
                            if !online.load(Ordering::Relaxed) {
                                log::info!("carrier ONLINE via relay {}", s.url());
                            }
                        }
                    }
                    let was = online.swap(any_connected, Ordering::Relaxed);
                    if was && !any_connected {
                        log::warn!("carrier relay connection lost — will re-join topics on recovery");
                    }
                    if let Some(r) = nrw.get() {
                        // udp_v4||udp_v6 => a direct UDP path is reachable, so the
                        // carrier can hole-punch and move data peer-to-peer; relay
                        // then only introduces. (Android currently reports false.)
                        direct.store(r.udp_v4 || r.udp_v6, Ordering::Relaxed);
                        udp_v4.store(r.udp_v4, Ordering::Relaxed);
                        udp_v6.store(r.udp_v6, Ordering::Relaxed);
                        // NAT-observed PUBLIC reflexive addresses (what the relay sees) → the device's
                        // real public IPv4 / IPv6 for the connection UI.
                        *observed_v4.lock().unwrap() = r.global_v4.map(|a| a.ip().to_string());
                        *observed_v6.lock().unwrap() = r.global_v6.map(|a| a.ip().to_string());
                        // keep the FULL reflexive addr (with port) for advertising
                        *reflex_v4.lock().unwrap() = r.global_v4.map(std::net::SocketAddr::from);
                        *reflex_v6.lock().unwrap() = r.global_v6.map(std::net::SocketAddr::from);
                        log::info!(
                            "net_report: preferred_relay={:?} udp_v4={} udp_v6={} pub_v4={:?} pub_v6={:?}",
                            r.preferred_relay, r.udp_v4, r.udp_v6, r.global_v4, r.global_v6
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            });
        }
        // ── Direct-P2P bridge (Android) ──────────────────────────────────────
        // Belt-and-suspenders on top of the patched netdev (which already heals
        // iroh's interface enumeration on Android via getifaddrs). We ALSO learn
        // our reachable addresses WITHOUT OS introspection — the relay-observed
        // public reflexive addr (network reflection), the connect-a-UDP-socket
        // source IP, and every interface addr (incl. a VPN tun overlay) — and
        // feed them to iroh as external addrs so peers can dial us DIRECTLY.
        // Purely additive: an unreachable candidate is ignored, so no regression
        // on hard-NAT networks.
        let direct_global = Arc::new(AtomicBool::new(false));
        {
            let ep = endpoint.clone();
            let direct_global = direct_global.clone();
            let reflex_v4 = reflex_v4.clone();
            let reflex_v6 = reflex_v6.clone();
            let advertised = advertised.clone();
            tokio::spawn(async move {
                let mut current: std::collections::BTreeSet<std::net::SocketAddr> = Default::default();
                loop {
                    let bound = ep.bound_sockets();
                    let port_v4 = bound.iter().find(|a| a.is_ipv4()).map(|a| a.port());
                    let port_v6 = bound.iter().find(|a| a.is_ipv6()).map(|a| a.port());
                    let port_any = bound.first().map(|a| a.port());
                    let mut desired: std::collections::BTreeSet<std::net::SocketAddr> = Default::default();
                    // Advertise EVERY usable address on EVERY interface (patched netdev now sees them
                    // on Android) — not just the one route-source IP. Critically this includes a
                    // WireGuard/VPN tun's OVERLAY address (e.g. 10.7.0.2): two phones on the same WG
                    // server are on one overlay subnet, so each dials the other's overlay IP DIRECTLY
                    // through the tunnel — no NAT, no hole-punch. Private/unreachable candidates are
                    // simply ignored by a peer that can't reach them, so this never regresses relay.
                    for iface in netdev::interface::get_interfaces() {
                        if iface.is_loopback() {
                            continue;
                        }
                        for n in &iface.ipv4 {
                            let a = n.addr();
                            if a.is_loopback() || a.is_link_local() || a.is_unspecified() {
                                continue;
                            }
                            if let Some(p) = port_v4.or(port_any) {
                                desired.insert(std::net::SocketAddr::new(std::net::IpAddr::V4(a), p));
                            }
                        }
                        for n in &iface.ipv6 {
                            let a = n.addr();
                            // skip loopback / unspecified / link-local (fe80::/10)
                            if a.is_loopback() || a.is_unspecified() || (a.segments()[0] & 0xffc0) == 0xfe80 {
                                continue;
                            }
                            if let Some(p) = port_v6.or(port_any) {
                                desired.insert(std::net::SocketAddr::new(std::net::IpAddr::V6(a), p));
                            }
                        }
                    }
                    // The NETWORK-learned public reflexive addrs (relay-observed,
                    // independent of Android interface enumeration) — the candidate
                    // a remote peer actually punches toward on cone NAT. This is the
                    // address Android can't hide, so the carrier stops depending on
                    // the OS knowing its own interfaces.
                    if let Some(sa) = *reflex_v4.lock().unwrap() {
                        desired.insert(sa);
                    }
                    if let Some(sa) = *reflex_v6.lock().unwrap() {
                        desired.insert(sa);
                    }
                    // Fallback: if enumeration somehow saw nothing, use the connect-probe source IP.
                    if desired.is_empty() {
                        if let Some(ip) = primary_ip(false) {
                            if let Some(p) = port_v4.or(port_any) {
                                desired.insert(std::net::SocketAddr::new(ip, p));
                            }
                        }
                        if let Some(ip) = primary_ip(true) {
                            if let Some(p) = port_v6.or(port_any) {
                                desired.insert(std::net::SocketAddr::new(ip, p));
                            }
                        }
                    }
                    if desired != current {
                        for a in current.difference(&desired) {
                            ep.remove_external_addr(a).await;
                        }
                        for a in desired.difference(&current) {
                            log::info!("carrier external addr += {a} (direct-P2P bridge)");
                            ep.add_external_addr(*a).await;
                        }
                        current = desired;
                        *advertised.lock().unwrap() = current.iter().map(|a| a.to_string()).collect();
                        ep.network_change().await;
                        log::info!(
                            "carrier direct-bridge: advertised={current:?} direct_global={}",
                            current.iter().any(|a| is_global_ip(&a.ip()))
                        );
                    }
                    // A globally-routable advertised address means a real direct
                    // path is achievable (vs LAN-only private addrs) — surface it.
                    direct_global.store(current.iter().any(|a| is_global_ip(&a.ip())), Ordering::Relaxed);
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            });
        }

        let broker = Arc::new(Mutex::new(load_broker(&dir).await));
        let known: std::collections::BTreeSet<String> = tokio::fs::read(dir.join("known-peers.json"))
            .await
            .ok()
            .and_then(|b| {
                // Sealed (DEK present) → open; legacy plaintext → parse raw and re-seal
                // on the next write. Wrong key/tamper → empty (rebuilt from live peers).
                if hey_core::plat::at_rest_active() && hey_core::crypto::is_at_rest(&b) {
                    hey_core::plat::open_with_at_rest_key(&b)
                } else {
                    Some(b)
                }
            })
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let carrier = Arc::new(Carrier {
            endpoint,
            gossip,
            _router: router,
            store,
            mem,
            senders: Mutex::new(HashMap::new()),
            neighbors: Arc::new(Mutex::new(HashMap::new())),
            broker,
            dir,
            online,
            direct,
            direct_global,
            known: Arc::new(Mutex::new(known)),
            observed_v4,
            observed_v6,
            udp_v4,
            udp_v6,
            advertised,
        });
        // Re-join persisted subscriptions so invites land even before the UI opens. Fold in EVERY
        // known peer so a restart (e.g. an app update) re-finds contacts even if a topic's own
        // bootstrap is thin.
        let subs: Vec<(String, Vec<String>)> = {
            let b = carrier.broker.lock().await;
            b.subscriptions.iter().map(|(t, v)| (t.clone(), v.clone())).collect()
        };
        let known0 = carrier.all_known().await;
        // Seed the in-RAM address store with every known peer's full addr BEFORE the
        // first dials, so the boot re-join resolves locally from the start (the dead
        // pkarr/DNS resolver on this network would otherwise only time out).
        carrier.reassert_known().await;
        for (topic, boot) in subs {
            carrier.ensure_topic(&topic, &boot).await;
            carrier.seed_peers(&topic, &known0).await;
        }
        // ── Cold-start burst re-dial ──────────────────────────────────────────────
        // The boot re-join above only QUEUES async dials; iroh's relay probe and
        // neighbor formation lag, so a topic can sit at ZERO neighbors for ~10s
        // after a (re)start — and a DM typed right after reopen gets queued, not
        // delivered (the "messages don't come after reopening" bug). Aggressively
        // re-dial every zero-neighbor topic on a tight schedule for the first few
        // seconds so messaging works almost instantly after a cold start.
        {
            let carrier = carrier.clone();
            tokio::spawn(async move {
                for delay_ms in [250u64, 400, 600, 900, 1300, 1900, 2800, 4000] {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    let subs: Vec<(String, Vec<String>)> = {
                        let b = carrier.broker.lock().await;
                        b.subscriptions.iter().map(|(t, v)| (t.clone(), v.clone())).collect()
                    };
                    let known = carrier.all_known().await;
                    // Keep every known peer's full addr in mem during the cold-start
                    // window so the first dials resolve locally (not via dead pkarr).
                    carrier.reassert_known().await;
                    for (topic, boot) in subs {
                        let n = carrier.neighbors.lock().await.get(&topic).map(|s| s.len()).unwrap_or(0);
                        if n == 0 {
                            carrier.rejoin_topic(&topic, &boot).await;
                            carrier.seed_peers(&topic, &known).await;
                        }
                    }
                }
            });
        }
        // ── Connectivity self-heal ────────────────────────────────────────────────
        // An internet drop (VPN, wifi↔cellular, tunnel, sleep) or a reboot leaves gossip topics with
        // zero neighbors, and iroh-gossip often never re-bootstraps them on its own. So we watch:
        //   (1) our primary IP for a change — the network-change signal Android won't hand iroh — and
        //       on change, re-probe + re-join EVERY topic; and
        //   (2) any subscribed topic sitting at zero neighbors while online — and re-join it.
        // Recovers within ~10s with no user action, even after a long outage. (An instant trigger
        // also comes from the Android connectivity callback via hey_net_changed.)
        {
            let carrier = carrier.clone();
            tokio::spawn(async move {
                let mut last_ip = primary_ip(false).or_else(|| primary_ip(true));
                let mut tick: u64 = 0;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    tick += 1;
                    let ip = primary_ip(false).or_else(|| primary_ip(true));
                    if ip != last_ip {
                        last_ip = ip;
                        carrier.network_changed().await;
                        continue;
                    }
                    // Skip ONLY when there is no network at all. Do NOT gate on
                    // relay-online: the relay can be slow to come up after a restart
                    // (or on a far/slow cellular link), and a zero-neighbor topic must
                    // keep re-dialing so it meshes the INSTANT a path appears.
                    if last_ip.is_none() {
                        continue;
                    }
                    let subs: Vec<(String, Vec<String>)> = {
                        let b = carrier.broker.lock().await;
                        b.subscriptions.iter().map(|(t, v)| (t.clone(), v.clone())).collect()
                    };
                    let known = carrier.all_known().await;
                    // Re-assert every known peer's full addr into mem on each heal
                    // tick: mem never expires, but a peer may have been added since
                    // boot (new contact) — this guarantees its addr is resolvable
                    // locally before the keep-alive/zero-neighbor re-dial below.
                    carrier.reassert_known().await;
                    // KEEP-ALIVE re-dial of EVERY topic every ~10s once a path is up
                    // (faster recovery after a simultaneous double-restart); a peer
                    // that reopened is re-found within this window. Zero-neighbor
                    // topics re-dial every 5s regardless of path state.
                    let path_up = carrier.is_online() || carrier.is_direct();
                    let keepalive = path_up && tick % 2 == 0;
                    for (topic, boot) in subs {
                        let n = carrier.neighbors.lock().await.get(&topic).map(|s| s.len()).unwrap_or(0);
                        if n == 0 || keepalive {
                            carrier.rejoin_topic(&topic, &boot).await;
                            carrier.seed_peers(&topic, &known).await;
                        }
                    }
                }
            });
        }
        Ok(carrier)
    }

    pub fn node_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// The shared iroh endpoint (so the voice module can dial a peer's `hey/voice/1` ALPN on it).
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint.clone()
    }

    /// Subscribe to an EPHEMERAL gossip topic for game presence (movement).
    /// Unlike `ensure_topic`, this touches NEITHER the broker NOR disk and is
    /// NOT recorded in `subscriptions`, so nothing persists and the topic is not
    /// re-joined on the next boot. The returned `GossipSender` is owned by the
    /// caller (verse_gossip), not stored in `self.senders`. Each received frame
    /// is decoded and handed to `on_msg(sender_did, content)`; the receiver loop
    /// dies as soon as `gen_now()` no longer equals `gen` (session/world moved on).
    pub async fn subscribe_ephemeral<G, F>(
        self: &Arc<Self>,
        topic: &str,
        bootstrap: &[String],
        gen: u64,
        gen_now: G,
        on_msg: F,
    ) -> Option<GossipSender>
    where
        G: Fn() -> u64 + Send + 'static,
        F: Fn(String, String, i64, String) + Send + 'static,
    {
        // decode_bootstrap injects relay+addrs into the in-RAM MemoryLookup so the
        // gossip swarm can dial these peers — but we deliberately do NOT call
        // remember_peers() (that persists to known-peers.json). This stays RAM-only.
        let ids: Vec<EndpointId> = bootstrap.iter().filter_map(|b| self.decode_bootstrap(b)).collect();
        let sub = match self.gossip.subscribe(topic_id(topic), ids).await {
            Ok(sub) => sub,
            Err(e) => {
                log::warn!("verse-gossip subscribe {topic} failed: {e}");
                return None;
            }
        };
        let (sender, mut receiver) = sub.split();
        tokio::spawn(async move {
            while let Some(ev) = receiver.next().await {
                if gen_now() != gen {
                    break; // session/world moved on → this loop is stale
                }
                if let Ok(Event::Received(msg)) = ev {
                    let (c, s, ts, g) = decode_wire(&msg.content);
                    on_msg(s, c, ts, g); // NO broker.append, NO fs::write — pure RAM hand-off
                }
            }
        });
        Some(sender)
    }

    /// True once iroh has reached a relay / learned its address.
    pub fn is_online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    /// True when a direct UDP path is available → data flows peer-to-peer and the
    /// relay only introduces. False → relay is carrying the encrypted data. Either
    /// iroh's own UDP probe succeeded, or our bridge advertised a routable addr.
    pub fn is_direct(&self) -> bool {
        self.direct.load(Ordering::Relaxed) || self.direct_global.load(Ordering::Relaxed)
    }

    /// The device's current IP stack, for the connection UI: (has IPv4, has a globally-routable
    /// IPv6 — i.e. `2000::/3`, NOT a ULA `fd00::`/`fc00::` which is NAT66'd and not directly
    /// reachable). A global IPv6 is what enables direct P2P. Reads REAL interface addresses (the
    /// patched netdev getifaddrs path works on Android, incl. a WireGuard/VPN tun's address) — the
    /// old UDP-connect probe didn't reliably surface a tunnel's IPv6 source. Falls back to the probe
    /// only if enumeration somehow returns nothing.
    pub fn net_stack(&self) -> (bool, bool) {
        let mut v4 = false;
        let mut v6_global = false;
        for iface in netdev::interface::get_interfaces() {
            if iface.is_loopback() {
                continue;
            }
            for n in &iface.ipv4 {
                let a = n.addr();
                if !a.is_link_local() && !a.is_loopback() && !a.is_unspecified() {
                    v4 = true;
                }
            }
            for n in &iface.ipv6 {
                if is_global_ip(&std::net::IpAddr::V6(n.addr())) {
                    v6_global = true;
                }
            }
        }
        if !v4 && !v6_global {
            v4 = primary_ip(false).is_some();
            v6_global = primary_ip(true).map(|ip| is_global_ip(&ip)).unwrap_or(false);
        }
        (v4, v6_global)
    }

    /// Concrete PUBLIC addresses for the connection UI: `(public_v6, public_v4)`. Prefers the
    /// NAT-observed reflexive address from net_report (the device's REAL public IP — incl. the NAT
    /// exit for IPv4); falls back to an interface global IPv6 (patched netdev) until net_report lands.
    pub fn net_addrs(&self) -> (Option<String>, Option<String>) {
        let mut v6 = self.observed_v6.lock().ok().and_then(|g| g.clone());
        let v4 = self.observed_v4.lock().ok().and_then(|g| g.clone());
        if v6.is_none() {
            for iface in netdev::interface::get_interfaces() {
                if iface.is_loopback() {
                    continue;
                }
                if let Some(n) = iface.ipv6.iter().find(|n| is_global_ip(&std::net::IpAddr::V6(n.addr()))) {
                    v6 = Some(n.addr().to_string());
                    break;
                }
            }
        }
        (v6, v4)
    }

    /// The net_report direct-UDP-path flags (v4, v6) — the connection screen's
    /// proof the agnostic discovery chain is live (true = iroh has a direct path).
    pub fn udp_paths(&self) -> (bool, bool) {
        (self.udp_v4.load(Ordering::Relaxed), self.udp_v6.load(Ordering::Relaxed))
    }

    /// The LOCAL addresses Hey advertises right now (interface + reflexive) —
    /// lets the UI show which interface Hey binds (WiFi vs VPN tun).
    pub fn advertised_addrs(&self) -> Vec<String> {
        self.advertised.lock().map(|v| v.clone()).unwrap_or_default()
    }

    /// HONEST per-peer path summary: (total_peers, direct_peers, relay_peers).
    /// A peer is "direct" only when iroh reports it currently has an ACTIVE
    /// non-relay (IP) transport path; "relay" when its only active path is a
    /// relay. Peers still negotiating count in neither. This is the real path of
    /// live connections — NOT the node-level `is_direct()` capability flag.
    pub async fn conn_summary(&self) -> (usize, usize, usize) {
        let mut ids = std::collections::HashSet::new();
        for s in self.neighbors.lock().await.values() {
            for n in s {
                ids.insert(n.clone());
            }
        }
        let total = ids.len();
        let (mut direct, mut relay) = (0usize, 0usize);
        for id_str in ids {
            let Ok(id) = id_str.parse::<EndpointId>() else { continue };
            if let Some(info) = self.endpoint.remote_info(id).await {
                let mut has_direct = false;
                let mut has_relay = false;
                for a in info.addrs() {
                    if matches!(a.usage(), iroh::endpoint::TransportAddrUsage::Active) {
                        if a.addr().is_relay() {
                            has_relay = true;
                        } else {
                            has_direct = true;
                        }
                    }
                }
                if has_direct {
                    direct += 1;
                } else if has_relay {
                    relay += 1;
                }
            }
        }
        (total, direct, relay)
    }

    /// Live transport to ONE peer by its node ticket: "direct" (an Active
    /// non-relay path), "relay" (only an Active relay path), or "offline"
    /// (unknown / no active path). Per-peer mirror of `conn_summary` — powers the
    /// per-contact direct/relay badge and the transport-gated attachment cap.
    pub async fn peer_transport(&self, ticket_or_id: &str) -> &'static str {
        let Some(id) = self.decode_bootstrap(ticket_or_id) else {
            return "offline";
        };
        let Some(info) = self.endpoint.remote_info(id).await else {
            return "offline";
        };
        let (mut has_direct, mut has_relay) = (false, false);
        for a in info.addrs() {
            if matches!(a.usage(), iroh::endpoint::TransportAddrUsage::Active) {
                if a.addr().is_relay() {
                    has_relay = true;
                } else {
                    has_direct = true;
                }
            }
        }
        if has_direct {
            "direct"
        } else if has_relay {
            "relay"
        } else {
            "offline"
        }
    }

    async fn save_broker(&self) {
        let snapshot = serialize_broker(&*self.broker.lock().await);
        if let Some(bytes) = snapshot {
            let _ = tokio::fs::write(self.dir.join("broker.json"), bytes).await;
        }
    }

    /// Decode a bootstrap ticket: base32 (upstream elastos-server) OR base64
    /// (legacy mobile / peer-provider) of a json EndpointAddr, or a bare
    /// EndpointId. Accepting BOTH encodings is what lets a mobile node and a
    /// VPS Hey-capsule mesh — same iroh/gossip/relay, the only delta was this.
    pub fn decode_bootstrap(&self, s: &str) -> Option<EndpointId> {
        let s = s.trim();
        let bytes = data_encoding::BASE32_NOPAD
            .decode(s.as_bytes())
            .ok()
            .or_else(|| B64.decode(s).ok());
        if let Some(bytes) = bytes {
            if let Ok(addr) = serde_json::from_slice::<EndpointAddr>(&bytes) {
                let id = addr.id;
                self.mem.add_endpoint_info(addr);
                return Some(id);
            }
        }
        EndpointId::from_str(s).ok()
    }

    /// Re-assert a ticket's FULL EndpointAddr (id + relay + direct IPs) into the
    /// in-RAM address store WITHOUT any of decode_bootstrap's other side-effects.
    ///
    /// This is the core of the local-first resolution fix: `self.mem` has NO TTL
    /// (verified in the vendored iroh-1.0-rc.1 `address_lookup::memory` — entries
    /// live in a plain BTreeMap and are only removed explicitly), and the endpoint
    /// queries ALL address-lookup services CONCURRENTLY and acts on the first item
    /// produced (`AddressLookupServices::resolve` merges streams; `MemoryLookup`
    /// yields its item synchronously). So whenever iroh-gossip dials a known peer
    /// BY ID on its own schedule (`Dialer::queue_dial` → `endpoint.connect(id)`),
    /// the address is returned IMMEDIATELY from mem and the dial proceeds — instead
    /// of falling through to n0's pkarr/DNS resolver, which can't be reached on this
    /// network and only times out. We re-assert on every re-dial cycle so the entry
    /// can never be missing when gossip dials (also: `add_endpoint_info` is additive
    /// for direct addrs and refreshes the relay URL, so re-asserting is cheap + safe).
    fn reassert_addr(&self, ticket: &str) {
        let s = ticket.trim();
        let bytes = data_encoding::BASE32_NOPAD
            .decode(s.as_bytes())
            .ok()
            .or_else(|| B64.decode(s).ok());
        if let Some(bytes) = bytes {
            if let Ok(addr) = serde_json::from_slice::<EndpointAddr>(&bytes) {
                // Only inject when there is something to resolve FROM — a bare id
                // with no relay/IP would just overwrite a richer entry's relay URL.
                if !addr.addrs.is_empty() {
                    self.mem.add_endpoint_info(addr);
                }
            }
        }
    }

    /// Re-assert the FULL EndpointAddr of EVERY known peer into the in-RAM store.
    /// Cheap (a base32/json decode + a map insert each) and idempotent; called at
    /// the top of every re-dial path so a gossip dial of any known contact ALWAYS
    /// resolves from mem first, never waiting on the dead pkarr/DNS resolver.
    async fn reassert_known(&self) {
        for t in self.known.lock().await.iter() {
            self.reassert_addr(t);
        }
    }

    /// Parse a ticket/bare-id into its EndpointId WITHOUT injecting the
    /// ticket's (possibly invite-time-stale) relay + socket addresses into the
    /// lookup. Realtime dials (voice, verse) address by IDENTITY and let live
    /// paths + discovery win — a stale relay URL from an old ticket otherwise
    /// poisons the dial right when it matters (the post-relay-switch silence).
    pub fn peer_id_of(&self, s: &str) -> Option<EndpointId> {
        let s = s.trim();
        let bytes = data_encoding::BASE32_NOPAD
            .decode(s.as_bytes())
            .ok()
            .or_else(|| B64.decode(s).ok());
        if let Some(bytes) = bytes {
            if let Ok(addr) = serde_json::from_slice::<EndpointAddr>(&bytes) {
                return Some(addr.id);
            }
        }
        EndpointId::from_str(s).ok()
    }

    async fn ensure_topic(self: &Arc<Self>, topic: &str, bootstrap: &[String]) {
        self.remember_peers(bootstrap).await;
        let ids: Vec<EndpointId> = bootstrap.iter().filter_map(|b| self.decode_bootstrap(b)).collect();
        {
            let mut s = self.senders.lock().await;
            if let Some(sender) = s.get(topic) {
                if !ids.is_empty() {
                    let _ = sender.join_peers(ids).await;
                }
                drop(s);
                // Fold NEW bootstrap tickets into the persisted subscription too:
                // a contact who reinstalled ships a fresh ticket, and the restart
                // re-join must dial the NEW address, not only the stale one.
                if !bootstrap.is_empty() {
                    let changed = {
                        let mut b = self.broker.lock().await;
                        let e = b.subscriptions.entry(topic.to_string()).or_default();
                        let mut changed = false;
                        for t in bootstrap {
                            if !e.contains(t) {
                                e.push(t.clone());
                                changed = true;
                            }
                        }
                        // newest wins: cap per-topic bootstrap so it can't grow forever
                        while e.len() > 64 {
                            e.remove(0);
                        }
                        changed
                    };
                    if changed {
                        self.save_broker().await;
                    }
                }
                return;
            }
            let sub = match self.gossip.subscribe(topic_id(topic), ids).await {
                Ok(sub) => sub,
                Err(e) => {
                    log::warn!("subscribe {topic} failed: {e}");
                    return;
                }
            };
            let (sender, mut receiver) = sub.split();
            s.insert(topic.to_string(), sender);

            let broker = self.broker.clone();
            let neighbors = self.neighbors.clone();
            let dir = self.dir.clone();
            let topic_s = topic.to_string();
            let carrier_weak = Arc::downgrade(self);
            tokio::spawn(async move {
                while let Some(ev) = receiver.next().await {
                    match ev {
                        Ok(Event::Received(msg)) => {
                            let (c, s, ts, g) = decode_wire(&msg.content);
                            // Only a real, user-facing DM should tick the locked/headless
                            // "new message" counter (the one that fires a notification while
                            // the app can't decrypt). Background control traffic must NOT:
                            //   - handshakes are ~23 KB → 8+ `hcfrag1` fragments (each was
                            //     firing its own "New message" with nothing to read),
                            //   - welcome / queue-rotation / profile / roster are single-shot
                            //     control wires with no ratchet header.
                            // A ratchet DM carries a top-level `"rh"`; fragments and control
                            // wires do not. (Cheap shape check — no decrypt, no key needed.)
                            let is_user_dm = !topic_s.starts_with("hey-social/feed")
                                && c.contains("\"rh\"")
                                && !c.contains("hcfrag1");
                            broker.lock().await.append(&topic_s, c, s, ts, g);
                            if is_user_dm {
                                INBOUND_COUNT.fetch_add(1, Ordering::Relaxed);
                            }
                            // Do NOT write the social graph in cleartext while the
                            // device is LOCKED (DEK cleared). The message stays in the
                            // in-RAM broker (INBOUND_COUNT already ticked) and is
                            // persisted SEALED on the next unlocked save. The host/CLI
                            // path has no DEK and is not "locked", so it still persists.
                            if !hey_core::plat::storage_locked() {
                                if let Some(bytes) = serialize_broker(&*broker.lock().await) {
                                    let _ = tokio::fs::write(dir.join("broker.json"), bytes).await;
                                }
                            }
                        }
                        Ok(Event::NeighborUp(id)) => {
                            neighbors.lock().await.entry(topic_s.clone()).or_default().insert(id.to_string());
                        }
                        Ok(Event::NeighborDown(id)) => {
                            let empty = {
                                let mut n = neighbors.lock().await;
                                if let Some(set) = n.get_mut(&topic_s) {
                                    set.remove(&id.to_string());
                                    set.is_empty()
                                } else {
                                    false
                                }
                            };
                            // A flap that drops the LAST neighbor → re-dial NOW,
                            // don't wait for the 5s self-heal. Short bursts re-form
                            // the mesh fast (bail as soon as a neighbor returns).
                            if empty {
                                if let Some(c) = carrier_weak.upgrade() {
                                    let topic = topic_s.clone();
                                    tokio::spawn(async move {
                                        for d in [200u64, 500, 1200] {
                                            tokio::time::sleep(std::time::Duration::from_millis(d)).await;
                                            if c.neighbors.lock().await.get(&topic).map(|s| s.len()).unwrap_or(0) > 0 {
                                                break;
                                            }
                                            // seed_peers re-dials the EXISTING sender (the topic
                                            // is live — we're in its receiver); NOT rejoin_topic,
                                            // which can re-enter ensure_topic and recursively
                                            // re-spawn this receiver (a non-Send future).
                                            let boot = c.broker.lock().await.subscriptions.get(&topic).cloned().unwrap_or_default();
                                            let known = c.all_known().await;
                                            c.seed_peers(&topic, &boot).await;
                                            c.seed_peers(&topic, &known).await;
                                        }
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
        // Persist the subscription for boot re-join.
        {
            let mut b = self.broker.lock().await;
            b.subscriptions.insert(topic.to_string(), bootstrap.to_vec());
        }
        self.save_broker().await;
    }

    /// Our shareable ticket = base32(json(EndpointAddr)) — id + relay + direct
    /// addrs. base32 matches the upstream elastos-server carrier so a Hey capsule
    /// on the VPS can decode our ticket (and vice-versa). decode_bootstrap still
    /// reads legacy base64, so old links keep working.
    async fn build_ticket(&self) -> String {
        // Online-gate the ticket so a boot-time invite always carries a working
        // relay URL. `endpoint.addr()` returns an EndpointAddr with no relay until
        // iroh has picked + connected a home relay; a relay-less ticket injected
        // into the peer's mem has neither a relay nor (after compaction) a direct
        // addr, so it is UNRESOLVABLE and the invite silently fails to mesh. Wait
        // (bounded) until a relay URL is present — but never block the caller
        // forever: if the relay is slow/down we still return the best addr we have
        // (direct IPs may carry it on a LAN), just past the wait budget.
        let has_relay = || self.endpoint.addr().relay_urls().next().is_some();
        if !has_relay() {
            // ~3s budget: poll the home-relay state; online() returns as soon as a
            // relay connects. Bounded so an offline device still gets a (best-effort)
            // ticket instead of hanging the share/QR UI.
            let _ = timeout(Duration::from_secs(3), self.endpoint.online()).await;
        }
        let addr = self.endpoint.addr();
        serde_json::to_vec(&addr)
            .map(|b| data_encoding::BASE32_NOPAD.encode(&b))
            .unwrap_or_default()
    }

    /// Recover after a network change (VPN flap, wifi↔cellular, long sleep): re-probe iroh's paths +
    /// relays, then RE-JOIN every subscribed topic with its bootstrap so gossip neighbors re-form
    /// promptly. iroh-gossip frequently does NOT re-bootstrap a topic on its own after a long drop —
    /// it just sits with zero neighbors — so an explicit re-join is what actually restores delivery.
    pub async fn network_changed(self: &Arc<Self>) {
        if IN_NET_CHANGE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return; // a re-probe is already in flight
        }
        self.endpoint.network_change().await;
        // A link change rebinds transports + resets the DNS resolver in iroh, but
        // it never touches OUR MemoryLookup (no TTL). Re-assert every known peer's
        // full addr anyway so the post-change re-dial resolves from mem instantly
        // rather than racing the just-reset (and on this network, dead) pkarr/DNS.
        self.reassert_known().await;
        let subs: Vec<(String, Vec<String>)> = {
            let b = self.broker.lock().await;
            b.subscriptions.iter().map(|(t, v)| (t.clone(), v.clone())).collect()
        };
        let n = subs.len();
        let known = self.all_known().await;
        for (topic, boot) in subs {
            self.rejoin_topic(&topic, &boot).await;
            self.seed_peers(&topic, &known).await;
        }
        log::info!("carrier: network changed → re-probed + re-joined {n} topic(s)");
        IN_NET_CHANGE.store(false, Ordering::SeqCst);
    }

    /// Re-seed a topic's bootstrap peers (re-dials them so neighbors re-form). Cheap + idempotent.
    async fn rejoin_topic(self: &Arc<Self>, topic: &str, bootstrap: &[String]) {
        let ids: Vec<EndpointId> = bootstrap.iter().filter_map(|b| self.decode_bootstrap(b)).collect();
        if ids.is_empty() {
            return;
        }
        if let Some(sender) = self.senders.lock().await.get(topic) {
            let _ = sender.join_peers(ids).await;
        } else {
            self.ensure_topic(topic, bootstrap).await;
        }
    }

    /// Remember peer tickets we've learned of (contacts/follows/group members/dials), persisted, so
    /// recovery can re-seed EVERY topic with the full set after a restart. Idempotent.
    async fn remember_peers(&self, peers: &[String]) {
        let mut changed = false;
        {
            let mut k = self.known.lock().await;
            for p in peers {
                let p = p.trim();
                // Inject the full EndpointAddr into the in-RAM store the moment we
                // learn it (a `connect`/`gossip_join_peers` ticket may only reach
                // gossip as a BARE id, so its addr must already be in mem when
                // gossip dials it by id — otherwise resolution falls to dead pkarr).
                self.reassert_addr(p);
                if !p.is_empty() && k.len() < 4096 && k.insert(p.to_string()) {
                    changed = true;
                }
            }
        }
        if changed {
            if let Ok(bytes) = serde_json::to_vec(&*self.known.lock().await) {
                // Seal the contact/follow/group social-graph + peer IPs at rest (mirror
                // broker.json). Plaintext only when no DEK exists (CLI/host harness).
                let sealed = hey_core::plat::seal_with_at_rest_key(&bytes).unwrap_or(bytes);
                let _ = tokio::fs::write(self.dir.join("known-peers.json"), sealed).await;
            }
        }
    }

    async fn all_known(&self) -> Vec<String> {
        self.known.lock().await.iter().cloned().collect()
    }

    /// Fold extra bootstrap peers into an ALREADY-joined topic without touching its persisted
    /// subscription — used to re-seed recovery with the whole known-peer set (a peer keeps the same
    /// endpoint id across restarts, so the relay re-resolves it even if its addresses changed).
    async fn seed_peers(&self, topic: &str, peers: &[String]) {
        let ids: Vec<EndpointId> = peers.iter().filter_map(|b| self.decode_bootstrap(b)).collect();
        if ids.is_empty() {
            return;
        }
        if let Some(sender) = self.senders.lock().await.get(topic) {
            let _ = sender.join_peers(ids).await;
        }
    }

    pub async fn handle(self: &Arc<Self>, op: &str, req: &Value) -> Value {
        match op {
            "init" => ok(json!({ "node_id": self.node_id(), "transport": "iroh-gossip" })),
            "get_ticket" | "my_ticket" => {
                ok(json!({ "ticket": self.build_ticket().await, "node_id": self.node_id() }))
            }
            "connect" => {
                let ticket = sf(req, "ticket");
                match self.decode_bootstrap(&ticket) {
                    Some(id) => {
                        self.remember_peers(&[ticket]).await; // survive a restart → re-find this peer
                        ok(json!({ "connected": [id.to_string()] }))
                    }
                    None => err("connect: undecodable ticket"),
                }
            }
            "gossip_join" => {
                let topic = sf(req, "topic");
                if topic.is_empty() {
                    return err("gossip_join: missing topic");
                }
                self.ensure_topic(&topic, &boot(req)).await;
                ok(json!({ "ok": true }))
            }
            "gossip_join_peers" => {
                let topic = sf(req, "topic");
                let peer_strs: Vec<String> = req
                    .get("peers")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                // Remember bare ids too: peers learned ONLY through this op (group
                // members, outbox dials) must survive a restart so recovery can
                // re-find them — a bare id still resolves via relay/pkarr lookup.
                self.remember_peers(&peer_strs).await;
                let peers: Vec<EndpointId> = peer_strs.iter().filter_map(|s| EndpointId::from_str(s).ok()).collect();
                self.ensure_topic(&topic, &[]).await;
                if let Some(sender) = self.senders.lock().await.get(&topic) {
                    if !peers.is_empty() {
                        let _ = sender.join_peers(peers).await;
                    }
                }
                ok(json!({ "ok": true }))
            }
            "gossip_leave" => ok(json!({ "ok": true })),
            "gossip_send" => {
                let topic = sf(req, "topic");
                if topic.is_empty() {
                    return err("gossip_send: missing topic");
                }
                let content = req
                    .get("message")
                    .or_else(|| req.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let sender_id = sf(req, "sender_id");
                let ts = nf(req, "ts");
                let signature = sf(req, "signature");
                let seq = self
                    .broker
                    .lock()
                    .await
                    .append(&topic, content.clone(), sender_id.clone(), ts, signature.clone());
                self.save_broker().await;
                self.ensure_topic(&topic, &[]).await;
                // Honest delivery report (failure must be loud, never silent-open):
                // a failed/impossible broadcast is `broadcast:"local_only"` — the
                // exact contract hey-core's outbox keys its retry queue on — and
                // `delivered` is true only when the broadcast went out with a live
                // neighbor to receive it. Success is never fabricated.
                let neighbors = self
                    .neighbors
                    .lock()
                    .await
                    .get(&topic)
                    .map(|s| s.len())
                    .unwrap_or(0);
                let mut broadcast = "ok";
                match self.senders.lock().await.get(&topic) {
                    Some(sender) => {
                        if let Err(e) = sender.broadcast(encode_wire(&content, &sender_id, ts, &signature)).await {
                            log::warn!("gossip_send {topic}: broadcast failed: {e}");
                            broadcast = "local_only";
                        }
                    }
                    None => broadcast = "local_only",
                }
                if broadcast == "ok" && neighbors == 0 && self.is_online() {
                    log::warn!(
                        "gossip_send {topic}: broadcast accepted but 0 neighbors while online                          — mesh not formed (peer on a different/unreachable relay?); outbox will retry"
                    );
                }
                ok(json!({
                    "seq": seq,
                    "delivered": broadcast == "ok" && neighbors > 0,
                    "broadcast": broadcast,
                    "neighbors": neighbors,
                }))
            }
            "gossip_recv" => {
                let topic = sf(req, "topic");
                if topic.is_empty() {
                    return err("gossip_recv: missing topic");
                }
                let limit = req.get("limit").and_then(Value::as_u64).unwrap_or(64).max(1) as usize;
                let consumer = sf(req, "consumer_id");
                if consumer.is_empty() {
                    return err("gossip_recv: missing consumer_id");
                }
                let skip = req.get("skip_sender_id").and_then(Value::as_str);
                let messages = self.broker.lock().await.drain(&topic, limit, &consumer, skip);
                if !messages.is_empty() {
                    self.save_broker().await;
                }
                ok(json!({ "messages": messages }))
            }
            "list_topic_peers" => {
                let topic = sf(req, "topic");
                let peers: Vec<String> = self
                    .neighbors
                    .lock()
                    .await
                    .get(&topic)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default();
                ok(json!({ "peers": peers }))
            }
            "list_peers" => {
                // Union of live gossip neighbors across all topics — the real
                // peer count for the status pill (carrier_health reads this).
                let mut set = std::collections::HashSet::new();
                for s in self.neighbors.lock().await.values() {
                    for n in s {
                        set.insert(n.clone());
                    }
                }
                ok(json!({ "peers": set.into_iter().collect::<Vec<_>>() }))
            }
            "peer_paths" => ok(json!({ "paths": {} })),
            other => err(format!("peer: unknown op {other}")),
        }
    }

    // ── blobs/* provider (cross-device media via iroh-blobs) ─────────────────────
    // Exact desktop blobs-provider wire contract (scheme "blobs"):
    //   add_bytes { data_base64 } -> { hash, ticket }
    //   fetch     { ticket }      -> { hash, bytes }      (base64 STD)
    // so hey-core's large-attachment path (Attachment.tickets, >16 KB chunked to 256 KiB)
    // works UNMODIFIED, and a mobile ticket is fetchable by a VPS box and vice-versa.
    pub async fn handle_blobs(&self, op: &str, req: &Value) -> Value {
        match op {
            "init" => ok(json!({ "node_id": self.node_id() })),
            "add_bytes" => {
                let data = match req.get("data_base64").and_then(Value::as_str).and_then(|s| B64_STD.decode(s).ok()) {
                    Some(d) => d,
                    None => return err("blobs.add_bytes: missing/!b64 data_base64"),
                };
                match self.blob_add(data).await {
                    Ok((hash, ticket)) => ok(json!({ "hash": hash, "ticket": ticket })),
                    Err(e) => err(format!("blobs.add_bytes: {e:#}")),
                }
            }
            "fetch" => {
                let ticket = sf(req, "ticket");
                if ticket.is_empty() {
                    return err("blobs.fetch: missing ticket");
                }
                match self.blob_fetch(&ticket).await {
                    Ok((hash, bytes)) => ok(json!({ "hash": hash, "bytes": B64_STD.encode(&bytes) })),
                    Err(e) => err(format!("blobs.fetch: {e:#}")),
                }
            }
            // share-by-hash needs the BlobFormat we don't persist → callers retain the
            // add_bytes ticket (same as desktop). FsStore runs no GC, so list/drop are
            // benign: a stored blob just persists.
            "share" => err("blobs.share: retain the ticket from add_bytes"),
            "list" => ok(json!({ "blobs": [] })),
            "drop" => ok(json!({ "ok": true })),
            other => err(format!("blobs: unknown op {other}")),
        }
    }

    /// Add bytes to the local blobs store; return (hash, ticket). The ticket carries OUR
    /// endpoint id, so a remote peer pulls the blob directly from us. Mirrors the desktop
    /// provider: stage to a temp, add_path (COPIES into the store), delete the temp —
    /// avoids depending on a streaming add_bytes API shape.
    async fn blob_add(&self, bytes: Vec<u8>) -> anyhow::Result<(String, String)> {
        let tmp = std::path::absolute(self.dir.join(format!("blob-add-{}.tmp", blake3::hash(&bytes).to_hex())))?;
        tokio::fs::write(&tmp, &bytes).await?;
        let res = self.store.blobs().add_path(tmp.clone()).await;
        let _ = tokio::fs::remove_file(&tmp).await;
        let tag = res?;
        let ticket = BlobTicket::new(self.endpoint.id().into(), tag.hash, tag.format);
        Ok((tag.hash.to_string(), ticket.to_string()))
    }

    /// Download the blob named by `ticket` directly P2P from its holder (unless we already
    /// hold it), then return its bytes inline — the WASM capsule shares no filesystem with
    /// us. Bounded so a dead holder can't hang the fetch. hey-core chunks to 256 KiB, so
    /// the export/read stays small.
    async fn blob_fetch(&self, ticket_str: &str) -> anyhow::Result<(String, Vec<u8>)> {
        let ticket: BlobTicket = ticket_str.parse()?;
        let hash = ticket.hash();
        if !self.store.blobs().has(hash).await.unwrap_or(false) {
            let downloader = self.store.downloader(&self.endpoint);
            let holder = ticket.addr().id;
            // Direct-P2P fetch with bounded RETRIES. One 30s shot with no retry
            // dropped the WHOLE file on any transient blip — and a large file is
            // many chunks, so the holder waking from background, a NAT re-bind, or
            // the carrier neighbor still forming would fail the fetch and the user
            // saw a chip that "does nothing". Retry a few times with backoff before
            // giving up; the holder must still be online (direct P2P, no relay/pin).
            // The FIRST fetch to a peer often fails because the direct QUIC path to
            // the holder hasn't formed yet (cold hole-punch) — the 2nd try works once
            // it's warm. So be patient: more retries with a longer ramp so the path
            // has time to come up WITHIN the first user tap, instead of erroring and
            // making them tap again. ~1+2+…+6 ≈ 21s of patience + the per-try timeout.
            let attempts = 6u32;
            let mut last_err: Option<anyhow::Error> = None;
            let mut ok = false;
            for attempt in 0..attempts {
                match timeout(
                    Duration::from_secs(30),
                    downloader.download(hash, Some(holder)).into_future(),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        ok = true;
                        break;
                    }
                    Ok(Err(e)) => last_err = Some(anyhow::anyhow!("blob download: {e}")),
                    Err(_) => {
                        last_err = Some(anyhow::anyhow!(
                            "blob download timed out (attempt {}/{})",
                            attempt + 1,
                            attempts
                        ))
                    }
                }
                tokio::time::sleep(Duration::from_millis(1000 * (attempt as u64 + 1))).await;
            }
            if !ok {
                return Err(last_err
                    .unwrap_or_else(|| anyhow::anyhow!("blob download failed (holder offline?)")));
            }
        }
        let tmp = std::path::absolute(self.dir.join(format!("blob-fetch-{}.tmp", hash)))?;
        self.store.blobs().export(hash, tmp.clone()).await?;
        let bytes = tokio::fs::read(&tmp).await?;
        let _ = tokio::fs::remove_file(&tmp).await;
        Ok((hash.to_string(), bytes))
    }
}

fn topic_id(topic: &str) -> TopicId {
    TopicId::from_bytes(*blake3::hash(topic.as_bytes()).as_bytes())
}

pub(crate) fn encode_wire(content: &str, sender_id: &str, ts: i64, signature: &str) -> Bytes {
    Bytes::from(json!({ "c": content, "s": sender_id, "t": ts, "g": signature }).to_string())
}
fn decode_wire(b: &[u8]) -> (String, String, i64, String) {
    if let Ok(v) = serde_json::from_slice::<Value>(b) {
        if v.get("c").is_some() {
            return (
                v.get("c").and_then(Value::as_str).unwrap_or("").into(),
                v.get("s").and_then(Value::as_str).unwrap_or("").into(),
                v.get("t").and_then(Value::as_i64).unwrap_or(0),
                v.get("g").and_then(Value::as_str).unwrap_or("").into(),
            );
        }
    }
    (String::from_utf8_lossy(b).into(), String::new(), 0, String::new())
}


/// Serialize the broker for on-disk storage. SEALS (ChaCha20-Poly1305, fresh
/// per-write nonce + magic header) when the at-rest DEK is installed; falls back
/// to plaintext only when no DEK exists (CLI / host harness). broker.json holds
/// buffered feed payloads + sender DID/ts/topic — the social graph — so it must
/// seal whenever the key is available. `None` == serialization failed.
fn serialize_broker(b: &Broker) -> Option<Vec<u8>> {
    let plain = serde_json::to_vec(b).ok()?;
    Some(hey_core::plat::seal_with_at_rest_key(&plain).unwrap_or(plain))
}

async fn load_broker(dir: &PathBuf) -> Broker {
    match tokio::fs::read(dir.join("broker.json")).await {
        Ok(b) => {
            // Sealed blob → open with the DEK (None on absent/wrong key or tamper
            // → empty buffer, rebuilt by live gossip, never garbage). Legacy
            // plaintext (no magic) parses raw and re-seals on the next write.
            if hey_core::plat::at_rest_active() && hey_core::crypto::is_at_rest(&b) {
                match hey_core::plat::open_with_at_rest_key(&b) {
                    Some(pt) => serde_json::from_slice(&pt).unwrap_or_default(),
                    None => Broker::default(),
                }
            } else {
                serde_json::from_slice(&b).unwrap_or_default()
            }
        }
        Err(_) => Broker::default(),
    }
}

// ── helpers shared with the rest of the runtime ──────────────────────────────

pub fn ok(data: Value) -> Value {
    json!({ "status": "ok", "data": data })
}
pub fn err(m: impl Into<String>) -> Value {
    json!({ "status": "error", "code": "peer", "message": m.into() })
}
fn sf(r: &Value, k: &str) -> String {
    r.get(k).and_then(Value::as_str).unwrap_or("").into()
}
fn nf(r: &Value, k: &str) -> i64 {
    r.get(k).and_then(Value::as_i64).unwrap_or(0)
}
fn boot(r: &Value) -> Vec<String> {
    r.get("bootstrap")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
