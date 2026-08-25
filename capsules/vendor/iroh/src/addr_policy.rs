//! HEY PATCH (additive, vendored-only) — the local address-candidate policy.
//!
//! # Why this lives HERE and not in the app
//!
//! Hyper's app layer (`hey-mobile-runtime::carrier`) has always filtered its own
//! advertised candidate set, and that filter has been rewritten many times. It never
//! held, because it is not the deciding layer: [`crate::socket`] independently
//! enumerates the host's interfaces (`collect_local_addresses`) and inserts every
//! address it finds as a `DirectAddrType::Local` candidate — below the app, with no
//! filtering at all. Those candidates go out in QUIC NAT-traversal frames regardless of
//! what the app decided. So every app-side fix closed one half of the leak and the other
//! half walked it straight back in. This module is that other half.
//!
//! # What the policy decides
//!
//! A candidate is worth advertising only if some peer could actually dial it:
//!
//! * **globally routable** (public IPv4, `2000::/3` IPv6) — any peer, anywhere;
//! * **LAN-scoped on a real subnet** (RFC1918 / ULA with a genuine prefix on a
//!   non-overlay, non-cellular interface) — a peer on that same segment.
//!
//! Everything else is noise that costs a remote peer a probe, a radio wake and a
//! timeout, and in the RFC1918 case can point them at an unrelated host on *their* LAN.
//! Typical offenders (fixture addresses, not a live lab):
//!
//! * `10.64.0.1/32` on `rmnet1` — a carrier-NAT (CGNAT) cellular address. The `/32`
//!   is the tell: an address with no local subnet can have no LAN peers *by definition*.
//! * `10.8.0.2/32` on `tun0` — a VPN overlay that is **not** the default route, so
//!   our traffic does not egress through it and the overlay address is dead.
//!
//! # The bias: KEEP when unsure
//!
//! Removing a dialable candidate breaks P2P for a real user; advertising a useless one
//! only wastes a probe. So every rule here fires only on positive evidence:
//!
//! * no interface context for an address (`iface == None`) ⇒ **keep**;
//! * `prefix_len == 0` means "the OS would not tell us" (the `getifaddrs` netmask
//!   fallback yields `0.0.0.0`, i.e. prefix 0 — it is *not* a real `/0`) ⇒ **keep**;
//! * a VPN overlay that *is* the default route (a full-tunnel WireGuard mesh) ⇒ **keep**,
//!   because same-mesh peers dial that overlay address directly;
//! * `default_route_interface` unknown ⇒ the overlay rule does not fire at all.
//!
//! # Invariant, not just a patch
//!
//! A vendored patch protects only until someone re-vendors the crate. The durable half of
//! this fix is [`AddrClass::is_advertisable`] being asserted at the single point of
//! advertisement in `Socket::update_direct_addresses` — if a non-dialable class ever
//! reaches that point again, it announces itself at `WARN` on the first run instead of
//! costing another week of bisecting. Addresses are never logged: only the class, the
//! family and the interface name.

use std::net::IpAddr;

/// The interface context needed to judge one local address.
///
/// Deliberately a borrowed view over whatever the platform layer has, rather than a
/// `netwatch`/`netdev` type: it keeps this module free of those crates, and lets the
/// pinning test in `hey-mobile-runtime` build synthetic interface sets that no test could
/// construct from `netwatch::interfaces::Interface` (its inner field is private).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IfaceView<'a> {
    /// The interface name, e.g. `wlan0`, `rmnet1`, `tun0`.
    pub name: &'a str,
    /// The prefix length of the address on this interface.
    ///
    /// `0` means UNKNOWN (the OS did not give us a netmask), **not** `/0`. Rules that
    /// depend on the prefix do not fire on `0`.
    pub prefix_len: u8,
    /// `Some(true)` if this interface carries the machine's default route, `Some(false)`
    /// if another interface does, `None` if the platform could not tell us.
    pub is_default_route: Option<bool>,
}

/// What a local address is, for the purpose of deciding whether to advertise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AddrClass {
    /// Globally routable IPv4. Advertise.
    GlobalV4,
    /// Globally routable IPv6 (`2000::/3`). Advertise.
    GlobalV6,
    /// RFC1918 IPv4 on a real subnet of a real interface. Advertise (same-LAN peers).
    LanV4,
    /// IPv6 ULA (`fc00::/7`) on a real interface. Advertise (same-LAN peers).
    LanV6,
    /// Private IPv4 behind a carrier NAT: a `/31`-or-longer prefix (no local subnet ⇒ no
    /// possible LAN peer) or a cellular interface. Undialable by anyone. Drop.
    CarrierNatV4,
    /// `100.64.0.0/10` shared address space (RFC 6598 CGNAT). Drop.
    CgnatV4,
    /// `192.0.0.0/24` — the 464XLAT CLAT / IETF protocol assignment block. Drop.
    ClatV4,
    /// An address on a VPN / point-to-point overlay that is provably NOT our egress path
    /// (a dead `tun0` while traffic actually leaves via cellular / Wi-Fi). Drop.
    OverlayInactive,
    /// `169.254.0.0/16` or `fe80::/10`. Needs a zone index to mean anything. Drop.
    LinkLocal,
    /// Loopback. Drop.
    Loopback,
    /// The unspecified address. Drop.
    Unspecified,
    /// Multicast, broadcast, documentation ranges and anything else not unicast. Drop.
    Reserved,
}

impl AddrClass {
    /// May an address of this class be advertised to remote peers as a direct candidate?
    ///
    /// This is the invariant asserted at the point of advertisement. Exactly two things
    /// qualify: globally routable, or LAN-scoped on a real (non-overlay) interface.
    pub const fn is_advertisable(self) -> bool {
        matches!(
            self,
            Self::GlobalV4 | Self::GlobalV6 | Self::LanV4 | Self::LanV6
        )
    }

    /// Is this class reachable from outside the local network segment?
    pub const fn is_global(self) -> bool {
        matches!(self, Self::GlobalV4 | Self::GlobalV6)
    }

    /// A short, log-safe name for this class. Safe to emit: it describes the *kind* of
    /// address, never the address.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GlobalV4 => "global-v4",
            Self::GlobalV6 => "global-v6",
            Self::LanV4 => "lan-v4",
            Self::LanV6 => "lan-v6",
            Self::CarrierNatV4 => "carrier-nat-v4",
            Self::CgnatV4 => "cgnat-v4",
            Self::ClatV4 => "clat-v4",
            Self::OverlayInactive => "overlay-inactive",
            Self::LinkLocal => "link-local",
            Self::Loopback => "loopback",
            Self::Unspecified => "unspecified",
            Self::Reserved => "reserved",
        }
    }
}

/// True if `name` is a VPN / point-to-point OVERLAY interface.
///
/// Kept byte-identical to `hey-mobile-runtime::carrier::is_vpn_overlay` on purpose: the
/// app half and this half must agree on what an overlay is, or they disagree about what
/// they are advertising and we are back to the recurring bug.
pub fn is_overlay_iface(name: &str) -> bool {
    name.starts_with("tun")
        || name.starts_with("utun")
        || name.starts_with("ppp")
        || name.starts_with("ipsec")
        || name.starts_with("wg")
}

/// True if `name` is a cellular / WWAN interface, where a private IPv4 is always a
/// carrier NAT assignment and never a LAN a peer could share.
///
/// `rmnet*` (Qualcomm), `ccmni*` (MediaTek), `pdp_ip*` (older Android / iOS), `seth_*`
/// (Exynos), plus the generic `wwan*` / `qmi*`. `rev_rmnet*` (reverse tether) is caught by
/// the `contains` arm.
pub fn is_cellular_iface(name: &str) -> bool {
    name.contains("rmnet")
        || name.starts_with("ccmni")
        || name.starts_with("pdp_ip")
        || name.starts_with("seth_")
        || name.starts_with("wwan")
        || name.starts_with("qmi")
}

/// Classify one local address, given whatever interface context the platform has.
///
/// `iface == None` means "we could not attribute this address to an interface"; the rules
/// that need interface context then do not fire, and the address is judged on its IP
/// class alone. That is the conservative direction: a private IPv4 with no known
/// interface stays [`AddrClass::LanV4`] and is advertised.
pub fn classify(ip: IpAddr, iface: Option<IfaceView<'_>>) -> AddrClass {
    // Normalise ::ffff:a.b.c.d so a v4-mapped address is judged as the v4 it is.
    let ip = ip.to_canonical();

    // ── Never dialable, on any interface, by anyone ──────────────────────────────
    if ip.is_loopback() {
        return AddrClass::Loopback;
    }
    if ip.is_unspecified() {
        return AddrClass::Unspecified;
    }
    if ip.is_multicast() {
        return AddrClass::Reserved;
    }
    match ip {
        IpAddr::V4(a) if a.is_link_local() => return AddrClass::LinkLocal,
        IpAddr::V6(a) if (a.segments()[0] & 0xffc0) == 0xfe80 => return AddrClass::LinkLocal,
        _ => {}
    }

    // ── A VPN overlay we do NOT egress through ───────────────────────────────────
    // Only fires on positive evidence that some OTHER interface owns the default route.
    // A full-tunnel overlay (is_default_route == Some(true)) falls through and is judged
    // normally, so a WireGuard mesh keeps advertising its overlay address to same-mesh
    // peers. An unknown default route (None) never triggers this rule.
    if let Some(i) = iface
        && is_overlay_iface(i.name)
        && i.is_default_route == Some(false)
    {
        return AddrClass::OverlayInactive;
    }

    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            if o[0] == 192 && o[1] == 0 && o[2] == 0 {
                return AddrClass::ClatV4; // 192.0.0.0/24 — CLAT / IETF protocol block
            }
            if o[0] == 100 && (o[1] & 0xc0) == 0x40 {
                return AddrClass::CgnatV4; // 100.64.0.0/10 — RFC 6598 shared space
            }
            if a.is_private() {
                // A private IPv4 is worth advertising only if a peer can be on the SAME
                // subnet. Two ways to know it cannot be:
                if let Some(i) = iface {
                    //  (a) the prefix leaves no room for neighbours. prefix_len == 0 is
                    //      "unknown", not "/0", so it must not trigger this.
                    if i.prefix_len >= 31 {
                        return AddrClass::CarrierNatV4;
                    }
                    //  (b) it is a cellular interface — a carrier NAT, never a LAN.
                    if is_cellular_iface(i.name) {
                        return AddrClass::CarrierNatV4;
                    }
                }
                return AddrClass::LanV4;
            }
            if a.is_broadcast() || a.is_documentation() {
                return AddrClass::Reserved;
            }
            AddrClass::GlobalV4
        }
        IpAddr::V6(a) => {
            let s = a.segments()[0];
            if (s & 0xfe00) == 0xfc00 {
                return AddrClass::LanV6; // fc00::/7 ULA
            }
            if (s & 0xe000) == 0x2000 {
                return AddrClass::GlobalV6; // 2000::/3 global unicast
            }
            AddrClass::Reserved
        }
    }
}

/// One rejected candidate, in a form that is safe to log: class + interface, no address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    /// Why it was rejected.
    pub class: AddrClass,
    /// The interface it was found on, or `"?"` if unattributed.
    pub iface: String,
    /// `true` for IPv6. Enough to read the log without the address.
    pub is_v6: bool,
}

/// The outcome of applying the policy to one enumeration of local addresses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The addresses that may be advertised, in input order.
    pub keep: Vec<IpAddr>,
    /// The addresses that were dropped, with the reason.
    pub rejected: Vec<Rejected>,
    /// True if the policy rejected EVERYTHING and `allow_fallback` was set, so `keep`
    /// holds the unfiltered input instead. A safety valve, never a normal outcome —
    /// callers must log it loudly.
    pub fell_back: bool,
}

/// Apply the policy to a set of local addresses.
///
/// `allow_fallback` should be true only when this is the caller's *last* source of
/// candidates (no port-mapped, QAD-observed or configured address is available). If the
/// policy would then leave the endpoint with nothing at all, the unfiltered set is
/// restored and [`Selection::fell_back`] is set. Rationale: a host whose interface
/// metadata we misread must degrade to "advertises something useless" and not to
/// "advertises nothing and is relay-only forever" — the second is the far worse bug.
pub fn select_advertisable<'a, I>(candidates: I, allow_fallback: bool) -> Selection
where
    I: IntoIterator<Item = (IpAddr, Option<IfaceView<'a>>)>,
{
    let mut keep = Vec::new();
    let mut rejected = Vec::new();
    let mut all: Vec<IpAddr> = Vec::new();

    for (ip, iface) in candidates {
        all.push(ip);
        let class = classify(ip, iface);
        if class.is_advertisable() {
            keep.push(ip);
        } else {
            rejected.push(Rejected {
                class,
                iface: iface.map(|i| i.name.to_string()).unwrap_or_else(|| "?".into()),
                is_v6: ip.is_ipv6(),
            });
        }
    }

    let fell_back = allow_fallback && keep.is_empty() && !all.is_empty();
    if fell_back {
        keep = all;
    }

    Selection {
        keep,
        rejected,
        fell_back,
    }
}
