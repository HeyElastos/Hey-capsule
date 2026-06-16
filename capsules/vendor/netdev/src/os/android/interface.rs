use super::netlink;
use crate::interface::interface::Interface;
use crate::interface::state::OperState;
use crate::ipnet::{Ipv4Net, Ipv6Net};
use crate::net::mac::MacAddr;
use std::net::{Ipv4Addr, Ipv6Addr};

#[cfg(feature = "gateway")]
use crate::net::device::NetworkDevice;
#[cfg(feature = "gateway")]
use crate::os::unix::dns::get_system_dns_conf;

use crate::os::unix::interface::unix_interfaces;

fn push_ipv4(v: &mut Vec<Ipv4Net>, add: (Ipv4Addr, u8)) {
    if v.iter()
        .any(|n| n.addr() == add.0 && n.prefix_len() == add.1)
    {
        return;
    }
    if let Ok(net) = Ipv4Net::new(add.0, add.1) {
        v.push(net);
    }
}

fn push_ipv6(v: &mut Vec<Ipv6Net>, add: (Ipv6Addr, u8)) -> bool {
    if v.iter()
        .any(|n| n.addr() == add.0 && n.prefix_len() == add.1)
    {
        return false;
    }
    if let Ok(net) = Ipv6Net::new(add.0, add.1) {
        v.push(net);
        return true;
    }
    false
}

#[inline]
fn calc_v6_scope_id(addr: &Ipv6Addr, ifindex: u32) -> u32 {
    let seg0 = addr.segments()[0];
    if (seg0 & 0xffc0) == 0xfe80 {
        ifindex
    } else {
        0
    }
}

fn finalize_interface(iface: &mut Interface) {
    if let Some(sysfs_type) = super::sysfs::get_interface_type(&iface.name) {
        iface.if_type = sysfs_type;
    } else if let Some(guessed_type) = super::types::guess_type_by_name(&iface.name) {
        iface.if_type = guessed_type;
    }

    if iface.transmit_speed.is_none() || iface.receive_speed.is_none() {
        let speed = super::sysfs::get_interface_speed(&iface.name);
        if iface.transmit_speed.is_none() {
            iface.transmit_speed = speed;
        }
        if iface.receive_speed.is_none() {
            iface.receive_speed = speed;
        }
    }

    if iface.stats.is_none() {
        iface.stats = crate::stats::counters::get_stats_from_name(&iface.name);
    }

    if iface.mtu.is_none() {
        iface.mtu = crate::os::linux::mtu::get_mtu(&iface.name);
    }
}

pub fn interfaces() -> Vec<Interface> {
    let mut ifaces: Vec<Interface> = Vec::new();

    // Hey patch: under Android SELinux the netlink RTM_GETLINK/RTM_GETADDR dump returns Ok(empty)
    // (a swallowed timeout), NOT Err — so upstream's `Err(_) => unix_interfaces()` getifaddrs
    // fallback was never reached, get_interfaces() returned [], and iroh saw zero interfaces and
    // disabled all UDP/QAD probing => relay-only. Treat an empty dump exactly like an error.
    let rows = netlink::collect_interfaces().unwrap_or_default();
    if rows.is_empty() {
        // fallback: unix ifaddrs (getifaddrs)
        ifaces = unix_interfaces();
    } else {
            for r in rows {
                let name = r.name.clone();
                let mut iface = Interface {
                    index: r.index,
                    name: name.clone(),
                    friendly_name: None,
                    description: None,
                    if_type: super::types::guess_type_by_name(&name).unwrap_or(r.if_type),
                    mac_addr: r.mac.map(MacAddr::from_octets),
                    ipv4: Vec::new(),
                    ipv6: Vec::new(),
                    ipv6_scope_ids: Vec::new(),
                    ipv6_addr_flags: Vec::new(),
                    flags: r.flags,
                    oper_state: OperState::from_if_flags(r.flags),
                    transmit_speed: None,
                    receive_speed: None,
                    auto_negotiate: None,
                    dhcp_v4_enabled: None,
                    dhcp_v6_enabled: None,
                    stats: r.stats.clone(),
                    #[cfg(feature = "gateway")]
                    gateway: None,
                    #[cfg(feature = "gateway")]
                    dns_servers: Vec::new(),
                    mtu: r.mtu,
                    #[cfg(feature = "gateway")]
                    default: false,
                };

                for (a, p) in r.ipv4 {
                    push_ipv4(&mut iface.ipv4, (a, p));
                }
                for (i, (a, p)) in r.ipv6.into_iter().enumerate() {
                    if push_ipv6(&mut iface.ipv6, (a, p)) {
                        iface.ipv6_scope_ids.push(calc_v6_scope_id(&a, iface.index));
                        let raw = r.ipv6_addr_flags.get(i).copied().unwrap_or(0);
                        iface
                            .ipv6_addr_flags
                            .push(crate::os::linux::ipv6_addr_flags::from_netlink_flags(raw));
                    }
                }

                ifaces.push(iface);
            }
    }

    for iface in &mut ifaces {
        finalize_interface(iface);
    }

    // Fill gateway info
    #[cfg(feature = "gateway")]
    {
        if let Ok(mut gmap) = netlink::collect_routes() {
            for iface in &mut ifaces {
                if iface.index == 0 {
                    continue;
                }
                if let Some(row) = gmap.remove(&iface.index) {
                    iface.gateway = Some(NetworkDevice {
                        mac_addr: row.mac.map(MacAddr::from_octets).unwrap_or(MacAddr::zero()),
                        ipv4: row.gw_v4,
                        ipv6: row.gw_v6,
                    });
                }
            }
        }

        if let Some(local_ip) = crate::net::ip::get_local_ipaddr() {
            if let Some(idx) = crate::interface::pick_default_iface_index(&ifaces, local_ip) {
                if let Some(iface) = ifaces.iter_mut().find(|it| it.index == idx) {
                    iface.default = true;
                    iface.dns_servers = get_system_dns_conf();
                }
            }
        }
    }

    // Hey patch (last resort): if BOTH netlink and getifaddrs came back blind (a hardened SELinux
    // domain where even getifaddrs is netlink-backed), synthesize ONE interface from the real
    // route-source IP so iroh still gets a dialable local candidate per family. get_local_ipaddr()
    // is a UDP-connect probe (bind UNSPECIFIED, connect to a public addr, read local_addr) — no
    // netlink, no getifaddrs, not SELinux-blocked. Returns None when truly offline, so we never
    // fabricate a phantom interface. The address is real (kernel-selected), so iroh advertises a
    // genuinely dialable candidate, enabling QAD + hole-punch instead of relay-only.
    if ifaces
        .iter()
        .all(|i| i.is_loopback() || (i.ipv4.is_empty() && i.ipv6.is_empty()))
    {
        #[cfg(feature = "gateway")]
        if let Some(ip) = crate::net::ip::get_local_ipaddr() {
            let mut syn = Interface::dummy();
            syn.index = 99;
            syn.name = "hey-synth0".to_string();
            // IFF_UP | IFF_RUNNING | IFF_MULTICAST (no IFF_LOOPBACK) → is_up()=true, is_loopback()=false.
            syn.flags = 0x1 | 0x40 | 0x1000;
            syn.oper_state = OperState::Up;
            match ip {
                std::net::IpAddr::V4(v4) => {
                    if let Ok(n) = Ipv4Net::new(v4, 24) {
                        syn.ipv4.push(n);
                    }
                }
                std::net::IpAddr::V6(v6) => {
                    if let Ok(n) = Ipv6Net::new(v6, 64) {
                        syn.ipv6.push(n);
                        syn.ipv6_scope_ids.push(0);
                        syn.ipv6_addr_flags
                            .push(crate::os::linux::ipv6_addr_flags::from_netlink_flags(0));
                    }
                }
            }
            ifaces.push(syn);
        }
    }

    ifaces
}
