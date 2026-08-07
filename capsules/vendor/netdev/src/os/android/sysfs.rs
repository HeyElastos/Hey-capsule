use crate::interface::types::InterfaceType;
use std::convert::TryFrom;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

// BATTERY: on modern Android, SELinux DENIES an untrusted app searching `/sys/class/net`
// (`avc: denied { search } ... name="net"`). Every `get_interfaces()` used to retry those denied
// reads per-interface — on a Pixel 10 that hammered the kernel audit path (~4-12 denied reads/sec)
// and, at boot/reconnect, contributed to a CPU burst. The sysfs reads here only ENRICH the
// interface type/speed, which the carrier doesn't rely on (addresses come from getifaddrs). So we
// probe `/sys/class/net` exactly ONCE and cache the result: if it's blocked, all further sysfs
// enrichment short-circuits to `None` with zero syscalls (one denial total instead of thousands).
static SYSFS_STATE: AtomicU8 = AtomicU8::new(0); // 0 = unknown, 1 = readable, 2 = blocked

fn sysfs_available() -> bool {
    match SYSFS_STATE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let ok = fs::read_dir("/sys/class/net").is_ok();
            SYSFS_STATE.store(if ok { 1 } else { 2 }, Ordering::Relaxed);
            ok
        }
    }
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}

fn exists(path: impl AsRef<Path>) -> bool {
    Path::new(path.as_ref()).exists()
}

fn is_wifi_interface(ifname: &str) -> bool {
    let base = PathBuf::from("/sys/class/net").join(ifname);

    if let Some(uevent) = read_trimmed(base.join("uevent")) {
        if uevent.lines().any(|line| line.trim() == "DEVTYPE=wlan") {
            return true;
        }
    }

    exists(base.join("wireless")) || exists(base.join("phy80211"))
}

pub fn get_interface_type(ifname: &str) -> Option<InterfaceType> {
    if !sysfs_available() {
        return None; // sysfs blocked by SELinux — skip enrichment, no repeated denied reads
    }
    if is_wifi_interface(ifname) {
        return Some(InterfaceType::Wireless80211);
    }

    let path = PathBuf::from("/sys/class/net").join(ifname).join("type");
    let value = read_trimmed(path)?.parse::<u32>().ok()?;

    if value == crate::os::linux::arp::ARPHRD_ETHER {
        Some(InterfaceType::Ethernet)
    } else {
        InterfaceType::try_from(value).ok()
    }
}

pub fn get_interface_speed(ifname: &str) -> Option<u64> {
    if !sysfs_available() {
        return None;
    }
    let path = PathBuf::from("/sys/class/net").join(ifname).join("speed");
    let speed_mbps = read_trimmed(path)?.parse::<i64>().ok()?;
    if speed_mbps <= 0 {
        return None;
    }

    Some((speed_mbps as u64) * 1_000_000)
}
