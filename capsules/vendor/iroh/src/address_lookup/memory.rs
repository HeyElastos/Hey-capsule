//! An in-memory address lookup system to manually add endpoint addressing information.
//!
//! Often an application might get endpoint addressing information out-of-band in an
//! application-specific way.  [`EndpointTicket`]'s are one common way used to achieve this.
//! This addressing information is often only usable for a limited time so needs to
//! be able to be removed again once you know it is no longer useful.
//!
//! This is where the [`MemoryLookup`] is useful: it allows applications to add and
//! retract endpoint addressing information that is otherwise out-of-band to iroh.
//!
//! [`EndpointTicket`]: https://docs.rs/iroh-tickets/latest/iroh_tickets/endpoint/struct.EndpointTicket.html

use std::{
    collections::{BTreeMap, HashMap, btree_map::Entry},
    sync::{Arc, RwLock},
};

use iroh_base::{EndpointId, TransportAddr};
use n0_future::{
    boxed::BoxStream,
    stream::{self, StreamExt},
    time::SystemTime,
};

use super::{AddressLookup, EndpointData, EndpointInfo, Error, Item};

/// An in-memory address lookup system to manually add endpoint addressing information.
///
/// Often an application might get endpoint addressing information out-of-band in an
/// application-specific way.  [`EndpointTicket`]'s are one common way used to achieve this.
/// This addressing information is often only usable for a limited time so needs to
/// be able to be removed again once you know it is no longer useful.
///
/// This is where the [`MemoryLookup`] is useful: it allows applications to add and
/// retract endpoint addressing information that is otherwise out-of-band to iroh.
///
/// # Examples
///
/// ```no_run
/// # #[cfg(with_crypto_provider)] // Endpoint::bind needs a crypto provider
/// # {
/// use iroh::{
///     Endpoint, EndpointAddr, TransportAddr, address_lookup::memory::MemoryLookup,
///     endpoint::presets,
/// };
/// use iroh_base::SecretKey;
///
/// # #[tokio::main]
/// # async fn wrapper() -> n0_error::Result<()> {
/// // Create the Address Lookup and endpoint.
/// let address_lookup = MemoryLookup::new();
///
/// let _ep = Endpoint::builder(presets::N0)
///     .address_lookup(address_lookup.clone())
///     .bind()
///     .await?;
///
/// // Sometime later add a RelayUrl for our endpoint.
/// let id = SecretKey::generate().public();
/// // You can pass either `EndpointInfo` or `EndpointAddr` to `add_endpoint_info`.
/// address_lookup.add_endpoint_info(EndpointAddr {
///     id,
///     addrs: [TransportAddr::Relay("https://example.com".parse()?)]
///         .into_iter()
///         .collect(),
/// });
///
/// # Ok(())
/// # }
/// # }
/// ```
///
/// [`EndpointTicket`]: https://docs.rs/iroh-tickets/latest/iroh_tickets/endpoint/struct.EndpointTicket.html
#[derive(Debug, Clone)]
pub struct MemoryLookup {
    endpoints: Arc<RwLock<BTreeMap<EndpointId, StoredEndpointInfo>>>,
    provenance: &'static str,
}

impl Default for MemoryLookup {
    fn default() -> Self {
        Self {
            endpoints: Default::default(),
            provenance: Self::PROVENANCE,
        }
    }
}

#[derive(Debug)]
struct StoredEndpointInfo {
    data: EndpointData,
    last_updated: SystemTime,
    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): when each address was last
    /// advertised to us.
    ///
    /// `last_updated` is per *endpoint*, so it says "we heard something about this peer" and
    /// cannot distinguish an address the peer still claims from one it abandoned three
    /// rebinds ago. This is the per-*address* clock that distinction needs. It is written
    /// unconditionally (cheap, and it means arming the switch mid-session has real
    /// timestamps to work from rather than starting everything at `now`), and read only when
    /// [`crate::radio::addr_freshness_enabled`] is on.
    ///
    /// Kept in step with `data` by [`StoredEndpointInfo::retain_fresh_addrs`], so it can
    /// never outgrow the address set it describes.
    addr_seen: HashMap<TransportAddr, SystemTime>,
}

impl StoredEndpointInfo {
    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): stamp every address in
    /// `addrs` as advertised at `now`.
    fn touch_addrs<'a>(&mut self, addrs: impl Iterator<Item = &'a TransportAddr>, now: SystemTime) {
        for addr in addrs {
            self.addr_seen.insert(addr.clone(), now);
        }
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): drop IP addresses this peer
    /// has stopped claiming, and bound how many it can claim at once.
    ///
    /// See [`crate::radio::ADDR_FRESHNESS`] for why this exists and what the rules are. The
    /// three properties this function must never violate, in order of severity:
    ///
    /// 1. **A non-IP address is never dropped.** Relay and custom transports are the
    ///    delivery backstop; losing one silently loses messages rather than saving packets.
    /// 2. **A peer with no non-IP backstop never has an IP aged out.** For such a peer this
    ///    store is the only route to them, so the staleness rule is skipped entirely and only
    ///    the cap applies.
    /// 3. **The address set never goes from non-empty to empty.** Guaranteed structurally by
    ///    (1) and by the cap being `>= 1`, and asserted by the unit tests rather than argued.
    ///
    /// Ordering is preserved for survivors: [`EndpointData`]'s own docs say address order can
    /// encode priority for lookup services, and an eviction pass has no business reordering
    /// what it did not remove.
    /// `max_ip` and `stale_after` are parameters rather than reads of [`crate::radio`] so the
    /// rules can be unit-tested without touching the process-global flag: the test binary is
    /// multi-threaded and flipping a global would race every other test that asserts the
    /// default-off behaviour. The flag itself is checked once, in
    /// [`StoredEndpointInfo::retain_fresh_addrs`].
    fn retain_fresh_addrs(&mut self, now: SystemTime) {
        if !crate::radio::addr_freshness_enabled() {
            return;
        }
        self.evict_addrs(now, crate::radio::ADDR_MAX_IP, crate::radio::ADDR_STALE_AFTER);
    }

    fn evict_addrs(
        &mut self,
        now: SystemTime,
        max_ip: usize,
        stale_after: std::time::Duration,
    ) -> usize {
        let have_backstop = self.data.addrs().any(|a| !a.is_ip());

        // Newest first. `sort_by_key` is stable, so addresses advertised in the same insert
        // keep the order the peer gave them in rather than being shuffled by the tie.
        let mut ips: Vec<(TransportAddr, SystemTime)> = self
            .data
            .addrs()
            .filter(|a| a.is_ip())
            .map(|a| {
                let seen = self.addr_seen.get(a).copied().unwrap_or(now);
                (a.clone(), seen)
            })
            .collect();
        ips.sort_by(|a, b| b.1.cmp(&a.1));

        let mut stale = 0usize;
        let mut capped = 0usize;
        let keep: std::collections::HashSet<TransportAddr> = ips
            .into_iter()
            .enumerate()
            .filter(|(rank, (_, seen))| {
                if *rank >= max_ip {
                    capped += 1;
                    return false;
                }
                // Only age out when something else can still carry a packet to this peer.
                if have_backstop {
                    let age = now.duration_since(*seen).unwrap_or_default();
                    if age >= stale_after {
                        stale += 1;
                        return false;
                    }
                }
                true
            })
            .map(|(_, (addr, _))| addr)
            .collect();

        if stale == 0 && capped == 0 {
            return 0;
        }

        // Rebuild in the original order, keeping every non-IP address untouched.
        let survivors: Vec<TransportAddr> = self
            .data
            .addrs()
            .filter(|a| !a.is_ip() || keep.contains(*a))
            .cloned()
            .collect();
        debug_assert!(
            !survivors.is_empty(),
            "address eviction must never empty a non-empty set"
        );
        let user_data = self.data.user_data().cloned();
        self.data = EndpointData::new(survivors);
        self.data.set_user_data(user_data);
        // Keep the clock in step with the set it describes, or `addr_seen` becomes the very
        // unbounded map this whole switch exists to prevent.
        self.addr_seen.retain(|a, _| !a.is_ip() || keep.contains(a));
        crate::radio::record_addrs_evicted(stale, capped);
        stale + capped
    }
}

impl MemoryLookup {
    /// The provenance string for this Address Lookup implementation.
    ///
    /// This is mostly used for debugging information and allows understanding the origin of
    /// addressing information used by an iroh [`Endpoint`].
    ///
    /// [`Endpoint`]: crate::Endpoint
    pub const PROVENANCE: &'static str = "memory_lookup";

    /// Creates a new empty Memory Lookup instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new Memory Lookup instance with the provided `provenance`.
    ///
    /// The provenance is part of [`address_lookup::Item`]s returned from [`Self::resolve`].
    /// It is mostly used for debugging information and allows understanding the origin of
    /// addressing information used by an iroh [`Endpoint`].
    ///
    /// [`Endpoint`]: crate::Endpoint
    /// [`address_lookup::Item`]: crate::address_lookup::Item
    pub fn with_provenance(provenance: &'static str) -> Self {
        Self {
            endpoints: Default::default(),
            provenance,
        }
    }

    /// Creates a Memory Lookup instance from endpoint addresses.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(with_crypto_provider)] // Endpoint::bind needs a crypto provider
    /// # {
    /// use std::{net::SocketAddr, str::FromStr};
    ///
    /// use iroh::{Endpoint, EndpointAddr, address_lookup::memory::MemoryLookup, endpoint::presets};
    ///
    /// # fn get_addrs() -> Vec<EndpointAddr> {
    /// #     Vec::new()
    /// # }
    /// # #[tokio::main]
    /// # async fn wrapper() -> n0_error::Result<()> {
    /// // get addrs from somewhere
    /// let addrs = get_addrs();
    ///
    /// // create a MemoryLookup from the list of addrs.
    /// let address_lookup = MemoryLookup::from_endpoint_info(addrs);
    /// // create an endpoint with the memory lookup address_lookup
    /// let endpoint = Endpoint::builder(presets::N0)
    ///     .address_lookup(address_lookup)
    ///     .bind()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn from_endpoint_info(infos: impl IntoIterator<Item = impl Into<EndpointInfo>>) -> Self {
        let res = Self::default();
        for info in infos {
            res.add_endpoint_info(info);
        }
        res
    }

    /// Sets endpoint addressing information for the given endpoint ID.
    ///
    /// This will completely overwrite any existing info for the endpoint.
    ///
    /// Returns the [`EndpointData`] of the previous entry, or `None` if there was no previous
    /// entry for this endpoint ID.
    pub fn set_endpoint_info(
        &self,
        endpoint_info: impl Into<EndpointInfo>,
    ) -> Option<EndpointData> {
        let last_updated = SystemTime::now();
        let EndpointInfo { endpoint_id, data } = endpoint_info.into();
        let mut guard = self.endpoints.write().expect("poisoned");
        let mut stored = StoredEndpointInfo {
            data,
            last_updated,
            addr_seen: HashMap::new(),
        };
        let addrs: Vec<_> = stored.data.addrs().cloned().collect();
        stored.touch_addrs(addrs.iter(), last_updated);
        stored.retain_fresh_addrs(last_updated);
        let previous = guard.insert(endpoint_id, stored);
        previous.map(|x| x.data)
    }

    /// Augments endpoint addressing information for the given endpoint ID.
    ///
    /// The provided addressing information is combined with the existing info in the memory
    /// lookup.  Any new direct addresses are added to those already present while the
    /// relay URL is overwritten.
    pub fn add_endpoint_info(&self, endpoint_info: impl Into<EndpointInfo>) {
        let last_updated = SystemTime::now();
        let EndpointInfo { endpoint_id, data } = endpoint_info.into();
        let mut guard = self.endpoints.write().expect("poisoned");
        match guard.entry(endpoint_id) {
            Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                existing.data.add_addrs(data.addrs().cloned());
                existing.data.set_user_data(data.user_data().cloned());
                existing.last_updated = last_updated;
                // HYPER PATCH (additive, flag-gated): stamp what this insert actually
                // carried, THEN evict. Stamping only the incoming addresses is the whole
                // mechanism — an address the peer no longer advertises keeps its old
                // timestamp and ages out, which is how a peer moving IPv4 -> IPv6 or landing
                // behind a NAT corrects itself without anyone being told.
                existing.touch_addrs(data.addrs(), last_updated);
                existing.retain_fresh_addrs(last_updated);
            }
            Entry::Vacant(entry) => {
                let mut stored = StoredEndpointInfo {
                    data,
                    last_updated,
                    addr_seen: HashMap::new(),
                };
                let addrs: Vec<_> = stored.data.addrs().cloned().collect();
                stored.touch_addrs(addrs.iter(), last_updated);
                // A first insert can already exceed the cap: a ticket may carry a large
                // address set. Bound it on arrival rather than waiting for a second insert.
                stored.retain_fresh_addrs(last_updated);
                entry.insert(stored);
            }
        }
    }

    /// Returns endpoint addressing information for the given endpoint ID.
    pub fn get_endpoint_info(&self, endpoint_id: EndpointId) -> Option<EndpointInfo> {
        let guard = self.endpoints.read().expect("poisoned");
        let info = guard.get(&endpoint_id)?;
        Some(EndpointInfo::from_parts(endpoint_id, info.data.clone()))
    }

    /// Removes all endpoint addressing information for the given endpoint ID.
    ///
    /// Any removed information is returned.
    pub fn remove_endpoint_info(&self, endpoint_id: EndpointId) -> Option<EndpointInfo> {
        let mut guard = self.endpoints.write().expect("poisoned");
        let info = guard.remove(&endpoint_id)?;
        Some(EndpointInfo::from_parts(endpoint_id, info.data))
    }
}

impl AddressLookup for MemoryLookup {
    fn publish(&self, _data: &EndpointData) {}

    fn resolve(&self, endpoint_id: EndpointId) -> Option<BoxStream<Result<super::Item, Error>>> {
        let guard = self.endpoints.read().expect("poisoned");
        let info = guard.get(&endpoint_id);
        match info {
            Some(endpoint_info) => {
                let last_updated = endpoint_info
                    .last_updated
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("time drift")
                    .as_micros() as u64;
                let item = Item::new(
                    EndpointInfo::from_parts(endpoint_id, endpoint_info.data.clone()),
                    self.provenance,
                    Some(last_updated),
                );
                Some(stream::iter(Some(Ok(item))).boxed())
            }
            None => None,
        }
    }
}

#[cfg(all(test, with_crypto_provider))]
mod tests {
    use iroh_base::{EndpointAddr, SecretKey, TransportAddr};
    use n0_error::{Result, StackResultExt};

    use super::*;
    use crate::{Endpoint, endpoint::presets};

    #[tokio::test]
    async fn test_basic() -> Result {
        let address_lookup = MemoryLookup::new();

        let _ep = Endpoint::builder(presets::Minimal)
            .address_lookup(address_lookup.clone())
            .bind()
            .await?;

        let key = SecretKey::from_bytes(&[0u8; 32]);
        let addr = EndpointAddr::from_parts(
            key.public(),
            [TransportAddr::Relay("https://example.com".parse()?)],
        );
        let user_data = Some("foobar".parse().unwrap());
        let endpoint_info = EndpointInfo::from(addr.clone()).with_user_data(user_data.clone());
        address_lookup.add_endpoint_info(endpoint_info.clone());

        let back = address_lookup
            .get_endpoint_info(key.public())
            .context("no addr")?;

        assert_eq!(back, endpoint_info);
        assert_eq!(back.user_data(), user_data.as_ref());
        assert_eq!(back.into_endpoint_addr(), addr);

        let removed = address_lookup
            .remove_endpoint_info(key.public())
            .context("nothing removed")?;
        assert_eq!(removed, endpoint_info);
        let res = address_lookup.get_endpoint_info(key.public());
        assert!(res.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_provenance() -> Result {
        let address_lookup = MemoryLookup::with_provenance("foo");
        let key = SecretKey::from_bytes(&[0u8; 32]);
        let addr = EndpointAddr::from_parts(
            key.public(),
            [TransportAddr::Relay("https://example.com".parse()?)],
        );
        address_lookup.add_endpoint_info(addr);
        let mut stream = address_lookup.resolve(key.public()).unwrap();
        let item = stream.next().await.unwrap()?;
        assert_eq!(item.provenance(), "foo");
        assert_eq!(
            item.relay_urls().next(),
            Some(&("https://example.com".parse()?))
        );

        Ok(())
    }
}

/// HYPER PATCH (additive): proof for the address-store freshness rules.
///
/// These call [`StoredEndpointInfo::evict_addrs`] with explicit parameters rather than
/// arming [`crate::radio::set_addr_freshness_enabled`], so they assert the *rules* without
/// racing the process-global flag that every other test in this binary expects to be off.
#[cfg(test)]
mod hey_addr_freshness {
    use std::{net::SocketAddr, time::Duration};

    use iroh_base::TransportAddr;
    use n0_future::time::SystemTime;

    use super::{EndpointData, HashMap, StoredEndpointInfo};

    const MAX_IP: usize = 6;
    const STALE: Duration = Duration::from_secs(1800);

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn ip(port: u16) -> TransportAddr {
        TransportAddr::Ip(format!("192.168.0.101:{port}").parse::<SocketAddr>().unwrap())
    }

    fn v6(port: u16) -> TransportAddr {
        TransportAddr::Ip(
            format!("[2a01:4ff:f0:b1b3::1]:{port}")
                .parse::<SocketAddr>()
                .unwrap(),
        )
    }

    fn relay() -> TransportAddr {
        TransportAddr::Relay("https://elastos.app".parse().unwrap())
    }

    /// Build a store entry whose addresses were each advertised at the given instant.
    fn stored(addrs: &[(TransportAddr, u64)]) -> StoredEndpointInfo {
        let mut addr_seen = HashMap::new();
        for (a, secs) in addrs {
            addr_seen.insert(a.clone(), t(*secs));
        }
        StoredEndpointInfo {
            data: EndpointData::new(addrs.iter().map(|(a, _)| a.clone()).collect()),
            last_updated: t(0),
            addr_seen,
        }
    }

    fn addrs_of(s: &StoredEndpointInfo) -> Vec<TransportAddr> {
        s.data.addrs().cloned().collect()
    }

    /// INVARIANT 1: the relay is the delivery backstop and is never eligible for eviction,
    /// however old it is and however many addresses are present.
    #[test]
    fn relay_is_never_evicted() {
        let mut s = stored(&[
            (relay(), 0),
            (ip(40001), 0),
            (ip(40002), 0),
            (ip(40003), 0),
            (ip(40004), 0),
            (ip(40005), 0),
            (ip(40006), 0),
            (ip(40007), 0),
        ]);
        // `now` is far beyond STALE for every address, including the relay's own timestamp.
        s.evict_addrs(t(100_000), MAX_IP, STALE);
        assert!(
            addrs_of(&s).contains(&relay()),
            "the relay must survive an eviction pass that drops every IP"
        );
        assert_eq!(
            addrs_of(&s).len(),
            1,
            "with a backstop present, every stale IP should be gone"
        );
    }

    /// INVARIANT 2: a peer known ONLY by IP (mDNS LAN neighbour, relay-less ticket) never has
    /// an address aged out — for that peer this store is the only route to them, so ageing is
    /// a disconnect, not a saving. The cap still applies.
    #[test]
    fn peer_with_no_backstop_never_ages_out() {
        let mut s = stored(&[(ip(40001), 0), (ip(40002), 0)]);
        s.evict_addrs(t(100_000), MAX_IP, STALE);
        assert_eq!(
            addrs_of(&s).len(),
            2,
            "no relay to fall back on, so ancient IPs must be kept"
        );
    }

    /// INVARIANT 3: eviction never turns a non-empty address set into an empty one.
    #[test]
    fn never_empties_a_non_empty_set() {
        for case in [
            vec![(ip(1), 0)],
            vec![(relay(), 0)],
            vec![(relay(), 0), (ip(1), 0)],
            vec![(ip(1), 0), (v6(2), 0)],
        ] {
            let mut s = stored(&case);
            s.evict_addrs(t(100_000), MAX_IP, STALE);
            assert!(
                !addrs_of(&s).is_empty(),
                "eviction emptied a non-empty set for {case:?}"
            );
        }
    }

    /// THE MEASURED PATHOLOGY: one peer, one host, eight UDP ports — one per socket rebind —
    /// every one of them drawing an equal share of the speculative fan-out. The cap bounds it
    /// to the freshest [`MAX_IP`], so the store is `O(contacts)` not `O(contacts x rebinds)`.
    #[test]
    fn cap_bounds_a_peer_that_rebinds_forever() {
        // Ports advertised oldest-to-newest, all recent enough that only the cap can bite.
        let mut s = stored(&[
            (relay(), 900),
            (ip(43457), 100),
            (ip(38558), 200),
            (ip(41429), 300),
            (ip(44540), 400),
            (ip(49835), 500),
            (ip(42500), 600),
            (ip(39905), 700),
            (ip(49842), 800),
        ]);
        s.evict_addrs(t(900), MAX_IP, STALE);
        let kept = addrs_of(&s);
        assert!(kept.contains(&relay()), "relay kept");
        let kept_ips: Vec<_> = kept.iter().filter(|a| a.is_ip()).collect();
        assert_eq!(kept_ips.len(), MAX_IP, "IP addresses bounded by the cap");
        // The two OLDEST rebinds are the ones dropped.
        assert!(!kept.contains(&ip(43457)), "oldest rebind dropped");
        assert!(!kept.contains(&ip(38558)), "second-oldest rebind dropped");
        assert!(kept.contains(&ip(49842)), "newest rebind kept");
    }

    /// ADAPTATION: a contact moves from IPv4 to IPv6. It simply stops advertising the v4
    /// address; nothing tells us. The v4 entry stops being refreshed and ages out on its own,
    /// while the re-advertised v6 address stays. No protocol change, no new packet.
    #[test]
    fn moving_from_v4_to_v6_self_corrects() {
        let mut s = stored(&[(relay(), 0), (ip(40001), 0)]);
        // Later the peer advertises only its v6 address. This is exactly what
        // `add_endpoint_info` does: union the new address in, stamp only what arrived.
        s.data.add_addrs([v6(40002)]);
        s.touch_addrs([v6(40002), relay()].iter(), t(1000));

        // Before the stale window elapses, BOTH are kept — we do not guess that the peer
        // moved, we wait for the evidence.
        s.evict_addrs(t(1000), MAX_IP, STALE);
        assert!(addrs_of(&s).contains(&ip(40001)), "v4 kept while still fresh");

        // The peer keeps re-advertising where it actually is now, as pkarr republication and
        // the app's own re-assert cycle both do. Only the abandoned v4 address stops being
        // refreshed, so only it accumulates age.
        s.touch_addrs([v6(40002), relay()].iter(), t(1000) + STALE);
        s.evict_addrs(t(1000) + STALE, MAX_IP, STALE);
        let kept = addrs_of(&s);
        assert!(!kept.contains(&ip(40001)), "abandoned v4 address aged out");
        assert!(kept.contains(&v6(40002)), "current v6 address kept");
        assert!(kept.contains(&relay()), "relay kept");
    }

    /// The same mechanism covers a peer that goes from publicly reachable to behind a NAT:
    /// the global address stops being advertised, the private one arrives, and the store
    /// follows the peer rather than accumulating both forever.
    #[test]
    fn moving_behind_a_nat_self_corrects() {
        let public: TransportAddr = TransportAddr::Ip("203.0.113.9:41000".parse().unwrap());
        let mut s = stored(&[(relay(), 0), (public.clone(), 0)]);
        s.data.add_addrs([ip(41001)]);
        s.touch_addrs([ip(41001), relay()].iter(), t(1000));
        // The peer goes on advertising its current private address; only the abandoned
        // public one stops being refreshed.
        s.touch_addrs([ip(41001), relay()].iter(), t(1000) + STALE);

        s.evict_addrs(t(1000) + STALE, MAX_IP, STALE);
        let kept = addrs_of(&s);
        assert!(!kept.contains(&public), "abandoned public address aged out");
        assert!(kept.contains(&ip(41001)), "current private address kept");
        assert!(kept.contains(&relay()), "relay kept");
    }

    /// Survivors keep their original order: `EndpointData`'s docs say address order can encode
    /// priority for lookup services, and an eviction pass has no business reordering what it
    /// did not remove.
    #[test]
    fn survivor_order_is_preserved() {
        let mut s = stored(&[
            (relay(), 900),
            (ip(1), 800),
            (ip(2), 100),
            (ip(3), 700),
            (ip(4), 600),
            (ip(5), 500),
            (ip(6), 400),
            (ip(7), 300),
        ]);
        s.evict_addrs(t(900), MAX_IP, STALE);
        let kept = addrs_of(&s);
        // ip(2) is the oldest and is the one dropped by the cap; the rest keep their order.
        assert_eq!(kept, vec![relay(), ip(1), ip(3), ip(4), ip(5), ip(6), ip(7)]);
    }

    /// The clock map must never outgrow the address set it describes, or it becomes the very
    /// unbounded structure this switch exists to prevent.
    #[test]
    fn clock_map_stays_in_step_with_the_address_set() {
        let mut s = stored(&[
            (relay(), 900),
            (ip(1), 100),
            (ip(2), 200),
            (ip(3), 300),
            (ip(4), 400),
            (ip(5), 500),
            (ip(6), 600),
            (ip(7), 700),
            (ip(8), 800),
        ]);
        s.evict_addrs(t(900), MAX_IP, STALE);
        assert_eq!(
            s.addr_seen.len(),
            addrs_of(&s).len(),
            "addr_seen must hold exactly the surviving addresses"
        );
    }

    /// With the flag off — the shipped default — the pass is a no-op even on a set that
    /// violates every rule. This is the rollback guarantee.
    #[test]
    fn disabled_by_default_is_a_no_op() {
        assert!(!crate::radio::addr_freshness_enabled());
        let mut s = stored(&[
            (relay(), 0),
            (ip(1), 0),
            (ip(2), 0),
            (ip(3), 0),
            (ip(4), 0),
            (ip(5), 0),
            (ip(6), 0),
            (ip(7), 0),
            (ip(8), 0),
        ]);
        s.retain_fresh_addrs(t(100_000));
        assert_eq!(addrs_of(&s).len(), 9, "nothing may change while unarmed");
    }
}
