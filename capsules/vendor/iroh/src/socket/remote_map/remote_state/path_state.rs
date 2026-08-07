//! The state kept for each network path to a remote endpoint.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use n0_error::e;
use n0_future::time::Instant;
use rustc_hash::FxHashMap;
use tokio::sync::oneshot;
use tracing::trace;

use super::{Source, TransportAddrInfo, TransportAddrUsage};
use crate::{address_lookup::AddressLookupFailed, metrics::SocketMetrics, socket::transports};

/// Maximum number of non-relay paths we keep around per endpoint.
pub(super) const MAX_NON_RELAY_PATHS: usize = 30;

/// Maximum number of inactive non-relay paths we keep around per endpoint.
///
/// These are paths that at one point been opened and are now closed.
pub(super) const MAX_INACTIVE_NON_RELAY_PATHS: usize = 10;

/// Map of all paths that we are aware of for a remote endpoint.
///
/// Also stores a list of resolve requests which are triggered once at least one path is known,
/// or once this struct is notified of a failed Address Lookup run.
#[derive(Debug)]
pub(super) struct RemotePathState {
    /// All possible paths we are aware of.
    ///
    /// These paths might be entirely impossible to use, since they are added by Address Lookup
    /// mechanisms. The are only potentially usable.
    paths: FxHashMap<transports::Addr, PathState>,
    /// Pending resolve requests from [`Self::resolve_remote`].
    pending_resolve_requests: VecDeque<oneshot::Sender<Result<(), AddressLookupFailed>>>,
    metrics: Arc<SocketMetrics>,
    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): rotation cursor for the
    /// speculative fan-out's re-probe window.
    ///
    /// Cold candidates are retried a few per send rather than all at once, and this is
    /// what walks the window across them so every candidate comes round within a bounded
    /// number of sends. It exists because [`Self::paths`] is an `FxHashMap`: its iteration
    /// order is arbitrary and per-process seeded, so any fixed-size selection taken from it
    /// could exclude the same live candidate forever. The cursor indexes a *sorted* view
    /// instead (see [`select_fanout_addrs`]), which makes coverage a proof rather than a
    /// probability.
    fanout_cursor: usize,
}

/// Describes the usability of this path, i.e. whether it has ever been opened,
/// when it was closed, or if it has never been usable.
#[derive(Debug, Default)]
pub(super) enum PathStatus {
    /// This path is open and active.
    Open,
    /// This path was once opened, but was abandoned at the given [`Instant`].
    Inactive(Instant),
    /// This path was never usable (we attempted holepunching and it didn't work).
    Unusable,
    /// We have not yet attempted holepunching, or holepunching is currently in
    /// progress, so we do not know the usability of this path.
    #[default]
    Unknown,
}

impl RemotePathState {
    pub(super) fn new(metrics: Arc<SocketMetrics>) -> Self {
        Self {
            paths: Default::default(),
            pending_resolve_requests: Default::default(),
            metrics,
            fanout_cursor: 0,
        }
    }

    pub(super) fn to_remote_addrs(&self) -> Vec<TransportAddrInfo> {
        self.paths
            .iter()
            .flat_map(|(addr, state)| {
                let usage = match state.status {
                    PathStatus::Open => TransportAddrUsage::Active,
                    PathStatus::Inactive(_) | PathStatus::Unusable | PathStatus::Unknown => {
                        TransportAddrUsage::Inactive
                    }
                };
                Some(TransportAddrInfo {
                    addr: addr.clone().into(),
                    usage,
                })
            })
            .collect()
    }

    /// Insert a new address of an open path into our list of paths.
    ///
    /// This will emit pending resolve requests and trigger pruning paths.
    pub(super) fn insert_open_path(&mut self, addr: transports::Addr, source: Source) {
        match addr {
            transports::Addr::Ip(_) => self.metrics.transport_ip_paths_added.inc(),
            transports::Addr::Relay(_, _) => self.metrics.transport_relay_paths_added.inc(),
            transports::Addr::Custom(_) => self.metrics.transport_custom_paths_added.inc(),
        };
        let state = self.paths.entry(addr).or_default();
        state.status = PathStatus::Open;
        // HYPER PATCH (additive): this path is carrying traffic again, so whatever we
        // recorded about it having stopped working is history. Clearing it here is what
        // makes the lifecycle sweep self-correcting: a path that comes back gets a fresh
        // start rather than being retired on the strength of a stale failure.
        state.unusable_since = None;
        // HYPER PATCH (additive): a path just opened on this address, which is the
        // strongest possible evidence that speculative datagrams to it are worth paying
        // for. Give it its full budget back. This is the per-address half of "re-arm on a
        // real signal"; the whole-table half is `rearm_fanout`.
        state.fanout_probes = 0;
        state.sources.insert(source.clone(), Instant::now());
        self.emit_pending_resolve_requests(None);
        self.prune_paths();
    }

    /// Mark a path as abandoned.
    ///
    /// If this path does not exist, it does nothing to the
    /// `RemotePathState`
    pub(super) fn abandoned_path(&mut self, addr: &transports::Addr) {
        if let Some(state) = self.paths.get_mut(addr) {
            if matches!(state.status, PathStatus::Open) {
                match addr {
                    transports::Addr::Ip(_) => self.metrics.transport_ip_paths_removed.inc(),
                    transports::Addr::Relay(_, _) => {
                        self.metrics.transport_relay_paths_removed.inc()
                    }
                    transports::Addr::Custom(_) => {
                        self.metrics.transport_custom_paths_removed.inc()
                    }
                };
            }
            match state.status {
                PathStatus::Open | PathStatus::Inactive(_) => {
                    state.status = PathStatus::Inactive(Instant::now());
                }
                PathStatus::Unusable | PathStatus::Unknown => {
                    state.status = PathStatus::Unusable;
                    // HYPER PATCH (additive): `Inactive` carries the instant it was
                    // abandoned; `Unusable` upstream carries nothing, so there is no clock
                    // on which a proven-dead path could ever be aged out. Stamp the first
                    // transition into `Unusable` and leave it alone afterwards, so the age
                    // measures "how long since we proved this does not work" rather than
                    // being reset by every repeat failure.
                    state.unusable_since.get_or_insert_with(Instant::now);
                }
            }
        }
    }

    /// Inserts multiple addresses of unknown status into our list of potential paths.
    ///
    /// If this caused the path set to transition from empty to non-empty, any
    /// pending resolve requests are woken with `Ok(())`. Inserts that add no
    /// new paths (empty iterator, or only duplicates) are a no-op: waking
    /// pending requests while the path set is still empty would send a bogus
    /// `AddressLookupFailed::NoResults` while an address lookup is in flight.
    pub(super) fn insert_multiple(
        &mut self,
        addrs: impl Iterator<Item = transports::Addr>,
        source: Source,
    ) {
        let now = Instant::now();
        let was_empty = self.paths.is_empty();
        for addr in addrs {
            self.paths
                .entry(addr)
                .or_default()
                .sources
                .insert(source.clone(), now);
        }
        trace!("added addressing information");
        if was_empty && !self.paths.is_empty() {
            self.emit_pending_resolve_requests(None);
        }
        self.prune_paths();
    }

    /// Sends back on `tx` once a possible path to the remote is known.
    ///
    /// If there already is a known path, `Ok(())` is returned immediately. Otherwise an
    /// address lookup is performed and the result is sent back once that
    /// completes. [`AddressLookupFailed`] is sent if there are no known paths.
    pub(super) fn resolve_remote(&mut self, tx: oneshot::Sender<Result<(), AddressLookupFailed>>) {
        if !self.paths.is_empty() {
            tx.send(Ok(())).ok();
        } else {
            self.pending_resolve_requests.push_back(tx);
        }
    }

    /// Returns `true` if there are any queued resolve requests from [`Self::resolve_remote`].
    pub(super) fn resolve_requests_is_empty(&self) -> bool {
        self.pending_resolve_requests.is_empty()
    }

    /// Notifies that a Address Lookup run has finished.
    ///
    /// This will emit pending resolve requests.
    pub(super) fn address_lookup_finished(&mut self, result: Result<(), AddressLookupFailed>) {
        self.emit_pending_resolve_requests(result.err());
    }

    /// Returns an iterator over the addresses of all paths.
    pub(super) fn addrs(&self) -> impl Iterator<Item = &transports::Addr> {
        self.paths.keys()
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): the destinations for one
    /// speculative send, i.e. a send made while we have no selected path.
    ///
    /// With the flag off this is exactly [`Self::addrs`] — every path, same order — so the
    /// send path is byte-for-byte upstream. With the flag on it applies the probe budget
    /// described on [`select_fanout_addrs`], and charges every destination it returns.
    ///
    /// Returns owned addresses rather than an iterator because charging the budget needs
    /// `&mut self` while the caller is still borrowing the result across an `.await`.
    pub(super) fn fanout_addrs(&mut self) -> Vec<transports::Addr> {
        if !crate::radio::fanout_budget_enabled() {
            return self.paths.keys().cloned().collect();
        }
        self.budgeted_fanout_addrs(
            crate::radio::FANOUT_PROBE_BUDGET,
            crate::radio::FANOUT_COLD_RETRIES,
            crate::radio::FANOUT_COLD_RETRIES_NO_RELAY,
        )
    }

    /// The budgeted half of [`Self::fanout_addrs`]: select, charge, and count.
    ///
    /// The limits are parameters rather than reads of [`crate::radio`] for the same reason
    /// [`retire_idle_non_relay_paths`]'s are: the test binary is multi-threaded, and
    /// flipping a process-global to exercise this would race every other test that asserts
    /// the default-off behaviour. The flag itself is checked once, in
    /// [`Self::fanout_addrs`].
    fn budgeted_fanout_addrs(
        &mut self,
        probe_budget: u32,
        cold_retries: usize,
        cold_retries_no_relay: usize,
    ) -> Vec<transports::Addr> {
        let (chosen, cursor) = select_fanout_addrs(
            &self.paths,
            self.fanout_cursor,
            probe_budget,
            cold_retries,
            cold_retries_no_relay,
        );
        self.fanout_cursor = cursor;
        // Charge the budget only for what we are actually about to send to. An address we
        // declined to probe has not had its trial, so it must not lose any of it. Relay and
        // custom addresses are not charged at all — "the relay is never budgeted" is then
        // true of the data and not merely of the selection, which is one less thing for the
        // next reader to have to take on trust.
        for addr in chosen.iter().filter(|a| a.is_ip()) {
            if let Some(state) = self.paths.get_mut(addr) {
                state.fanout_probes = state.fanout_probes.saturating_add(1);
            }
        }
        crate::radio::record_fanout(chosen.len(), self.paths.len().saturating_sub(chosen.len()));
        chosen
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): give every candidate its
    /// full probe budget back.
    ///
    /// Called on a link change and nothing else. That is the one event which invalidates
    /// every reachability judgement we hold at once — the addresses we proved unreachable
    /// were proved unreachable *on a network that no longer exists*. Every other re-arm is
    /// per-address and evidence-driven (see [`PathState::fanout_probes`]).
    ///
    /// A no-op with the flag off, and a no-op when nothing was cold, so it is safe to call
    /// unconditionally from the signal site.
    pub(super) fn rearm_fanout(&mut self) -> usize {
        if !crate::radio::fanout_budget_enabled() {
            return 0;
        }
        self.rearm_fanout_now()
    }

    /// The unconditional half of [`Self::rearm_fanout`] — see
    /// [`Self::budgeted_fanout_addrs`] for why the flag check is kept out of here.
    fn rearm_fanout_now(&mut self) -> usize {
        let mut rearmed = 0;
        for state in self.paths.values_mut() {
            if state.fanout_probes > 0 {
                state.fanout_probes = 0;
                rearmed += 1;
            }
        }
        if rearmed > 0 {
            trace!(rearmed, "fan-out budget: re-armed after a link change");
        }
        rearmed
    }

    /// Returns whether this stores any addresses.
    pub(super) fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Replies to all pending resolve requests.
    ///
    /// This is a no-op if no requests are queued. Replies `Ok` if we have any known paths,
    /// otherwise with the provided `address_lookup_error` or with [`AddressLookupFailed::NoResults`].
    fn emit_pending_resolve_requests(&mut self, address_lookup_error: Option<AddressLookupFailed>) {
        if self.pending_resolve_requests.is_empty() {
            return;
        }
        let result = match (self.paths.is_empty(), address_lookup_error) {
            (false, _) => Ok(()),
            (true, Some(err)) => Err(err),
            (true, None) => Err(e!(AddressLookupFailed::NoResults { errors: Vec::new() })),
        };
        for tx in self.pending_resolve_requests.drain(..) {
            tx.send(result.clone()).ok();
        }
    }

    /// Prune paths.
    ///
    /// Should be invoked any time we insert a new path.
    ///
    /// We currently only prune non-relay paths. For more information on the
    /// criteria for when and which paths we prune, look at the [`prune_non_relay_paths`] function.
    pub(super) fn prune_paths(&mut self) {
        prune_non_relay_paths(&mut self.paths);
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): retire paths that have
    /// stopped being useful, rather than waiting for cap pressure that may never come.
    ///
    /// `in_use` is the set of remote addresses that are live paths on some connection right
    /// now. It is passed in rather than derived from [`PathStatus`] on purpose: the status
    /// can lag reality (closing a whole connection does not mark its paths abandoned), and
    /// retiring a path we are actually sending on would be a delivery bug, not an
    /// efficiency win.
    ///
    /// Returns how many paths were retired. See [`retire_idle_non_relay_paths`] for the
    /// rules, and note that with the flag off this is an unconditional `0`.
    pub(super) fn retire_idle_paths(&mut self, in_use: &HashSet<transports::Addr>) -> usize {
        if !crate::radio::path_lifecycle_enabled() {
            return 0;
        }
        retire_idle_non_relay_paths(
            &mut self.paths,
            in_use,
            Instant::now(),
            crate::radio::PATH_RETIRE_IDLE_AFTER,
            crate::radio::PATH_RETIRE_SPARES,
        )
    }

    /// Number of paths currently tracked, for tests and instrumentation.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.paths.len()
    }
}

/// The state of a single path to the remote endpoint.
///
/// Each path is identified by the destination [`transports::Addr`] and they are stored in
/// the [`RemotePathState`] map in [`RemoteStateActor`].
///
/// [`RemoteStateActor`]: super::RemoteStateActor
#[derive(Debug, Default)]
pub(super) struct PathState {
    /// How we learned about this path, and when.
    ///
    /// We keep track of only the latest [`Instant`] for each [`Source`], keeping the size
    /// of the map of sources down to one entry per type of source.
    pub(super) sources: HashMap<Source, Instant>,
    /// The usability status of this path.
    pub(super) status: PathStatus,
    /// HYPER PATCH (additive): when this path was first proven unusable.
    ///
    /// [`PathStatus::Inactive`] already carries the instant it was abandoned.
    /// [`PathStatus::Unusable`] carries nothing, so upstream has no clock on which a path
    /// that failed holepunching could ever be aged out — it can only ever be evicted under
    /// cap pressure. This is that missing clock, and it is the only new state the adaptive
    /// lifecycle needs.
    ///
    /// `None` for a path that has never been proven unusable, including one that was
    /// unusable and has since come back (see [`RemotePathState::insert_open_path`]).
    pub(super) unusable_since: Option<Instant>,
    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): speculative datagrams
    /// spent on this address since the last evidence that it is worth spending on.
    ///
    /// "Speculative" means sent by [`super::State::handle_msg_send_datagram`]'s
    /// no-selected-path fan-out, i.e. sent on the hope that this address answers rather
    /// than on the knowledge that it does. Once this reaches
    /// [`crate::radio::FANOUT_PROBE_BUDGET`] the address has had a full connect ladder and
    /// answered none of it, and it stops being paid for on every burst — see
    /// [`select_fanout_addrs`].
    ///
    /// Reset to zero by evidence, never by a timer: a path opening on this address
    /// ([`RemotePathState::insert_open_path`]) re-arms this address, and a link change
    /// ([`RemotePathState::rearm_fanout`]) re-arms every address, because a link change is
    /// the one event that invalidates all of our reachability knowledge at once.
    ///
    /// Deliberately NOT re-armed by an address being re-advertised: the pkarr publisher
    /// republishes every five minutes, so treating a re-advertisement as news would re-arm
    /// the whole table on a schedule and quietly undo the whole thing. A genuinely new
    /// address arrives as a fresh [`PathState`] and so starts at zero for free.
    pub(super) fanout_probes: u32,
}

/// Prunes the non-relay paths in the paths HashMap.
///
/// Only prunes if the number of non-relay paths is above [`MAX_NON_RELAY_PATHS`].
///
/// Keeps paths that are open or of unknown status.
///
/// Always prunes paths that have unsuccessfully holepunched.
///
/// Keeps [`MAX_INACTIVE_NON_RELAY_PATHS`] of the most recently closed paths
/// that are not currently being used but have successfully been
/// holepunched previously.
///
/// This all ensures that:
///
/// - We do not have unbounded growth of paths.
/// - If we have many paths for this remote, we prune the paths that cannot hole punch.
/// - We do not prune holepunched paths that are currently not in use too quickly. For example, if a large number of untested paths are added at once, we will not immediately prune all of the unused, but valid, paths at once.
fn prune_non_relay_paths(paths: &mut FxHashMap<transports::Addr, PathState>) {
    // if the total number of paths is less than the max, bail early
    if paths.len() < MAX_NON_RELAY_PATHS {
        return;
    }

    let primary_paths: Vec<_> = paths.iter().filter(|(addr, _)| !addr.is_relay()).collect();

    // if the total number of non-relay paths is less than the max, bail early
    if primary_paths.len() < MAX_NON_RELAY_PATHS {
        return;
    }

    // paths that were opened at one point but have previously been closed
    let mut inactive = Vec::with_capacity(primary_paths.len());
    // paths where we attempted hole punching but it not successful
    let mut failed = Vec::with_capacity(primary_paths.len());

    for (addr, state) in primary_paths {
        match state.status {
            PathStatus::Inactive(t) => {
                // paths where holepunching succeeded at one point, but the path was closed.
                inactive.push((addr.clone(), t));
            }
            PathStatus::Unusable => {
                // paths where holepunching has been attempted and failed.
                failed.push(addr.clone());
            }
            _ => {
                // ignore paths that are open or the status is unknown
            }
        }
    }

    // All paths are bad, don't prune all of them.
    //
    // This implies that `inactive` is empty.
    if failed.len() == paths.len() {
        // leave the max number of non-relay paths
        failed.truncate(paths.len().saturating_sub(MAX_NON_RELAY_PATHS));
    }

    // sort the potentially prunable from most recently closed to least recently closed
    inactive.sort_by_key(|b| std::cmp::Reverse(b.1));

    // Prune the "oldest" closed paths.
    let old_inactive =
        inactive.split_off(inactive.len().saturating_sub(MAX_INACTIVE_NON_RELAY_PATHS));

    // collect all the paths that should be pruned
    let must_prune: HashSet<_> = failed
        .into_iter()
        .chain(old_inactive.into_iter().map(|(addr, _)| addr))
        .collect();

    paths.retain(|addr, _| !must_prune.contains(addr));
}

/// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): the idleness half of pruning.
///
/// # Why this exists
///
/// [`prune_non_relay_paths`] is the only pruning upstream has, and it is purely
/// cap-driven: it returns immediately unless the table is already at
/// [`MAX_NON_RELAY_PATHS`], and it is only ever called from the two insert paths. Together
/// that means **a path that stops working is never retired — it is only evicted to make
/// room for a newer one**. The steady state is "hold the maximum forever, mostly dead",
/// which is what the on-device path census showed: ten paths to one peer, one of them
/// actually carrying traffic.
///
/// That is not just untidy. When there is no selected path, `handle_msg_send_datagram`
/// sends the datagram to *every* address in this table, so dead entries are paid for in
/// real transmissions.
///
/// # The rules
///
/// A path is retired only if **all** of the following hold:
///
/// * it is an IP path. Relay paths are never touched — the relay is the delivery backstop
///   that every gossip broadcast rides, so losing it silently loses messages. Custom
///   transports are an embedder concern and are left alone too.
/// * it is not live on any connection (`in_use`), whatever its recorded status says.
/// * we have positive evidence it stopped working: [`PathStatus::Inactive`] (was open, then
///   abandoned) or [`PathStatus::Unusable`] (holepunching was tried and failed). A path
///   that is [`PathStatus::Open`], or that we have simply never tried
///   ([`PathStatus::Unknown`]), is never retired here — untried candidates are dial hints,
///   and throwing them away would slow down exactly the reconnects this is meant to keep
///   fast. Those remain governed by the cap-pressure prune above.
/// * it has been out of service for at least [`radio::PATH_RETIRE_IDLE_AFTER`].
/// * it is not one of the [`radio::PATH_RETIRE_SPARES`] most-recently-idle such paths.
///
/// The spare count is what makes "never prune the last usable path" structural rather than
/// a rule someone has to remember: it is a floor on survivors, applied before anything is
/// removed. Two further belt-and-braces guards below refuse to empty the table or to leave
/// a remote with no path at all when it has no relay path to fall back on.
///
/// Re-widening is deliberately NOT here. This function only ever narrows; widening happens
/// because a real signal — link change, new candidate, path established, last direct path
/// lost — makes the actor stand this sweep down (see `RemoteStateActor::arm_path_widening`)
/// while holepunching repopulates the table.
///
/// [`radio::PATH_RETIRE_IDLE_AFTER`]: crate::radio::PATH_RETIRE_IDLE_AFTER
/// [`radio::PATH_RETIRE_SPARES`]: crate::radio::PATH_RETIRE_SPARES
/// `idle_after` and `spares` are parameters rather than reads of [`crate::radio`] so this can
/// be unit-tested without touching the process-global flag: the test binary is
/// multi-threaded, and flipping a global would race every other test that asserts the
/// default-off behaviour. The flag itself is checked once, in
/// [`RemotePathState::retire_idle_paths`].
fn retire_idle_non_relay_paths(
    paths: &mut FxHashMap<transports::Addr, PathState>,
    in_use: &HashSet<transports::Addr>,
    now: Instant,
    idle_after: Duration,
    spares: usize,
) -> usize {
    // Everything that could be retired, paired with the instant it stopped being useful.
    let mut candidates: Vec<(transports::Addr, Instant)> = paths
        .iter()
        .filter(|(addr, _)| addr.is_ip())
        .filter(|(addr, _)| !in_use.contains(*addr))
        .filter_map(|(addr, state)| out_of_service_since(state).map(|t| (addr.clone(), t)))
        .filter(|(_, since)| {
            now.checked_duration_since(*since).unwrap_or_default() >= idle_after
        })
        .collect();

    // Keep the most recently idle ones as warm spares: sort oldest-first and drop from the
    // front, so what survives is always the freshest knowledge we have.
    candidates.sort_by_key(|(_, since)| *since);
    let retire_count = candidates.len().saturating_sub(spares);
    if retire_count == 0 {
        return 0;
    }
    candidates.truncate(retire_count);

    // Belt and braces: NEVER prune the last usable path. This is unreachable while
    // `PATH_RETIRE_SPARES >= 1` (which `radio`'s own test pins), because the spare floor
    // above already holds paths back. It is checked anyway rather than argued, because the
    // cost of the argument being wrong one day is a peer we can no longer send to — and
    // this is the cheapest possible place to be certain.
    if paths.len() <= retire_count {
        trace!("path lifecycle: refusing to empty the path table");
        return 0;
    }

    let retire: HashSet<_> = candidates.into_iter().map(|(addr, _)| addr).collect();
    paths.retain(|addr, _| !retire.contains(addr));
    trace!(retired = retire.len(), "path lifecycle: retired idle paths");
    retire.len()
}

/// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): pick the destinations for one
/// speculative send.
///
/// # What "speculative" means, and why this loop is expensive
///
/// [`super::State::handle_msg_send_datagram`] has two modes. With a selected path it sends
/// one copy to that path. Without one it sends a copy to *every* address in the table,
/// because any of them might be the one that works. On a CGNAT handset that table saturates
/// at [`MAX_NON_RELAY_PATHS`] dead private candidates, so every QUIC connect pays
/// `30 x 8` datagrams — the Initial and its PTO ladder — for a connect that completes over
/// the relay. Measured on device: 3.649 pkt/s and 66% of all bytes transmitted at idle.
///
/// # The rules
///
/// Three tiers, in this order:
///
/// 1. **Every non-IP address, unconditionally.** Relay and custom addresses are the
///    delivery backstop — the relay leg is what every gossip broadcast and every first
///    contact with a NAT'd peer actually rides — so they are never subject to a budget,
///    never rotated, and never counted against any cap. This tier is why narrowing the
///    fan-out cannot cost a message.
/// 2. **Every IP candidate that still has budget.** A candidate under
///    `probe_budget` speculative datagrams has not yet had a full connect ladder, so it is
///    still unproven rather than proven silent, and it is dialled exactly as upstream would
///    dial it. First contact is therefore unchanged.
/// 3. **A rotating window of cold IP candidates.** Candidates that spent a whole ladder
///    without ever answering are not abandoned — they are re-probed `cold_retries` at a
///    time, walking a cursor over a *sorted* list so coverage is a proof: every cold
///    candidate is re-probed at least once every `ceil(cold / cold_retries)` sends.
///
/// The sort is load-bearing. `paths` is an `FxHashMap` and its iteration order is arbitrary
/// and per-process seeded; selecting a window from that order could exclude the same live
/// candidate on every single send, for the whole life of the process, turning a slow
/// connect into a permanent failure. Sorting by [`transports::Addr`]'s own `Ord` makes the
/// window walk a stable sequence, so "tried within a bounded number of rounds" is
/// guaranteed rather than hoped for.
///
/// # When there is no backstop
///
/// If the table holds no relay and no custom address, this loop is the *only* way a packet
/// ever reaches this remote — an mDNS-discovered LAN peer, or a ticket whose relay was
/// filtered out. Tier 1 is then empty and narrowing would be narrowing the delivery path
/// itself, so the window widens to `cold_retries_no_relay`, sized so a single connect ladder
/// still sweeps a full saturated table. First contact is no slower than upstream; it is
/// merely spread across the ladder instead of shouted at every destination at once.
///
/// Returns the chosen destinations and the next cursor value.
fn select_fanout_addrs(
    paths: &FxHashMap<transports::Addr, PathState>,
    cursor: usize,
    probe_budget: u32,
    cold_retries: usize,
    cold_retries_no_relay: usize,
) -> (Vec<transports::Addr>, usize) {
    let mut chosen = Vec::with_capacity(paths.len().min(16));
    let mut cold: Vec<&transports::Addr> = Vec::new();
    let mut have_backstop = false;

    for (addr, state) in paths.iter() {
        if !addr.is_ip() {
            // Tier 1: relay and custom transports, always.
            have_backstop = true;
            chosen.push(addr.clone());
        } else if state.fanout_probes < probe_budget {
            // Tier 2: still within its trial.
            chosen.push(addr.clone());
        } else {
            cold.push(addr);
        }
    }

    if cold.is_empty() {
        return (chosen, cursor);
    }

    // Tier 3. Widen when tier 1 was empty: with no backstop this fan-out *is* delivery.
    let window = if have_backstop {
        cold_retries
    } else {
        cold_retries_no_relay
    }
    // Never zero. A zero window would turn "re-probe a few" into "abandon them all", and a
    // peer that came back on the LAN would never be found again without a link change.
    .max(1)
    .min(cold.len());

    // Stable order, so the rotation is a sweep rather than a lottery over map iteration.
    cold.sort();
    let start = cursor % cold.len();
    for i in 0..window {
        chosen.push(cold[(start + i) % cold.len()].clone());
    }

    // Advance by the window so successive sends cover disjoint slices and the sweep
    // completes in `ceil(cold.len() / window)` sends. Kept modulo `cold.len()` so the
    // cursor cannot drift out of range as the table grows and shrinks under it.
    let next = (start + window) % cold.len();

    debug_assert!(
        !chosen.is_empty(),
        "speculative fan-out must never be empty while paths exist"
    );
    (chosen, next)
}

/// When a path stopped being useful, or `None` if it is in service or was never tried.
///
/// `Open` and `Unknown` deliberately return `None`: the first is carrying traffic, and the
/// second is an untried dial hint rather than a known-bad path.
fn out_of_service_since(state: &PathState) -> Option<Instant> {
    match state.status {
        PathStatus::Inactive(since) => Some(since),
        PathStatus::Unusable => state.unusable_since,
        PathStatus::Open | PathStatus::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{Ipv4Addr, SocketAddrV4},
        time::Duration,
    };

    use iroh_base::{RelayUrl, SecretKey};
    use rand::{RngExt, SeedableRng};

    use super::*;

    fn ip_addr(port: u16) -> transports::Addr {
        transports::Addr::Ip(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into())
    }

    fn path_state_inactive(closed: Instant) -> PathState {
        PathState {
            sources: HashMap::new(),
            status: PathStatus::Inactive(closed),
            unusable_since: None,
            fanout_probes: 0,
        }
    }

    fn path_state_unusable() -> PathState {
        PathState {
            sources: HashMap::new(),
            status: PathStatus::Unusable,
            unusable_since: None,
            fanout_probes: 0,
        }
    }

    /// An unusable path with a known age, for the lifecycle tests.
    fn path_state_unusable_since(since: Instant) -> PathState {
        PathState {
            sources: HashMap::new(),
            status: PathStatus::Unusable,
            unusable_since: Some(since),
            fanout_probes: 0,
        }
    }

    fn relay_addr(seed: u64) -> transports::Addr {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let relay_url: RelayUrl = url::Url::parse("https://localhost")
            .expect("should be valid url")
            .into();
        transports::Addr::Relay(relay_url, SecretKey::from_bytes(&rng.random()).public())
    }

    /// Retire with the shipped constants but an explicit `now`, so tests are deterministic.
    fn retire(paths: &mut FxHashMap<transports::Addr, PathState>, now: Instant) -> usize {
        retire_idle_non_relay_paths(
            paths,
            &HashSet::new(),
            now,
            crate::radio::PATH_RETIRE_IDLE_AFTER,
            crate::radio::PATH_RETIRE_SPARES,
        )
    }

    /// Comfortably past [`crate::radio::PATH_RETIRE_IDLE_AFTER`].
    fn long_ago(now: Instant, extra_secs: u64) -> Instant {
        now - crate::radio::PATH_RETIRE_IDLE_AFTER - Duration::from_secs(extra_secs)
    }

    /// `(ip_active, ip_inactive, relay_active, relay_inactive)` — computed exactly the way
    /// the on-device `carrier pathcensus` line computes it, from the same
    /// [`RemotePathState::to_remote_addrs`] output, so the numbers here are directly
    /// comparable to the ones in the log.
    fn census(state: &RemotePathState) -> (usize, usize, usize, usize) {
        let mut counts = (0, 0, 0, 0);
        for info in state.to_remote_addrs() {
            let active = matches!(info.usage, TransportAddrUsage::Active);
            match info.addr {
                iroh_base::TransportAddr::Ip(_) if active => counts.0 += 1,
                iroh_base::TransportAddr::Ip(_) => counts.1 += 1,
                iroh_base::TransportAddr::Relay(_) if active => counts.2 += 1,
                iroh_base::TransportAddr::Relay(_) => counts.3 += 1,
                _ => {}
            }
        }
        counts
    }

    #[test]
    fn test_prune_under_max_paths() {
        let mut paths = FxHashMap::default();
        for i in 0..20 {
            paths.insert(ip_addr(i), PathState::default());
        }

        prune_non_relay_paths(&mut paths);
        assert_eq!(
            20,
            paths.len(),
            "should not prune when under MAX_NON_RELAY_PATHS"
        );
    }

    #[test]
    fn test_prune_at_max_paths_no_prunable() {
        let mut paths = FxHashMap::default();
        // All paths are active (never abandoned), so none should be pruned
        for i in 0..MAX_NON_RELAY_PATHS {
            paths.insert(ip_addr(i as u16), PathState::default());
        }

        prune_non_relay_paths(&mut paths);
        assert_eq!(
            MAX_NON_RELAY_PATHS,
            paths.len(),
            "should not prune active paths"
        );
    }

    #[test]
    fn test_prune_failed_holepunch() {
        let mut paths = FxHashMap::default();

        // Add 20 active paths
        for i in 0..20 {
            paths.insert(ip_addr(i), PathState::default());
        }

        // Add 15 failed holepunch paths (must_prune)
        for i in 20..35 {
            paths.insert(ip_addr(i), path_state_unusable());
        }

        prune_non_relay_paths(&mut paths);

        // All failed holepunch paths should be pruned
        assert_eq!(20, paths.len());
        for i in 0..20 {
            assert!(paths.contains_key(&ip_addr(i)));
        }
        for i in 20..35 {
            assert!(!paths.contains_key(&ip_addr(i)));
        }
    }

    #[test]
    fn test_prune_keeps_most_recent_inactive() {
        let mut paths = FxHashMap::default();
        let now = Instant::now();

        // Add 15 active paths
        for i in 0..15 {
            paths.insert(ip_addr(i), PathState::default());
        }

        // Add 20 inactive paths with different abandon times
        // Ports 15-34, with port 34 being most recently abandoned
        for i in 0..20 {
            let abandoned_time = now - Duration::from_secs((20 - i) as u64);
            paths.insert(ip_addr(15 + i as u16), path_state_inactive(abandoned_time));
        }

        assert_eq!(35, paths.len());
        prune_non_relay_paths(&mut paths);

        // Should keep 15 active + 10 most recently abandoned
        assert_eq!(25, paths.len());

        // Active paths should remain
        for i in 0..15 {
            assert!(paths.contains_key(&ip_addr(i)));
        }

        // Most recently abandoned (ports 25-34) should remain
        for i in 25..35 {
            assert!(paths.contains_key(&ip_addr(i)), "port {} should be kept", i);
        }

        // Oldest abandoned (ports 15-24) should be pruned
        for i in 15..25 {
            assert!(
                !paths.contains_key(&ip_addr(i)),
                "port {} should be pruned",
                i
            );
        }
    }

    #[test]
    fn test_prune_mixed_must_and_can_prune() {
        let mut paths = FxHashMap::default();
        let now = Instant::now();

        // Add 15 active paths
        for i in 0..15 {
            paths.insert(ip_addr(i), PathState::default());
        }

        // Add 5 failed holepunch paths
        for i in 15..20 {
            paths.insert(ip_addr(i), path_state_unusable());
        }

        // Add 15 usable but abandoned paths
        for i in 0..15 {
            let abandoned_time = now - Duration::from_secs((15 - i) as u64);
            paths.insert(ip_addr(20 + i as u16), path_state_inactive(abandoned_time));
        }

        assert_eq!(35, paths.len());
        prune_non_relay_paths(&mut paths);

        // Remove all failed paths -> down to 30
        // Keep MAX_INACTIVE_NON_RELAY_PATHS, eg remove 5 usable but abandoned paths -> down to 20
        assert_eq!(20, paths.len());

        // Active paths should remain
        for i in 0..15 {
            assert!(paths.contains_key(&ip_addr(i)));
        }

        // Failed holepunch should be pruned
        for i in 15..20 {
            assert!(!paths.contains_key(&ip_addr(i)));
        }

        // Most recently abandoned (ports 30-34) should remain
        for i in 30..35 {
            assert!(paths.contains_key(&ip_addr(i)), "port {} should be kept", i);
        }
    }

    #[test]
    fn test_prune_relay_paths_not_counted() {
        let mut paths = FxHashMap::default();

        // Add 25 IP paths (under MAX_NON_RELAY_PATHS)
        for i in 0..25 {
            paths.insert(ip_addr(i), path_state_unusable());
        }

        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0u64);
        let relay_url: RelayUrl = url::Url::parse("https://localhost")
            .expect("should be valid url")
            .into();
        // Add 10 relay addresses
        for _ in 0..10 {
            let id = SecretKey::from_bytes(&rng.random()).public();
            let relay_addr = transports::Addr::Relay(relay_url.clone(), id);
            paths.insert(relay_addr, PathState::default());
        }

        assert_eq!(35, paths.len()); // 25 IP + 10 relay
        prune_non_relay_paths(&mut paths);

        // Should not prune since non-relay paths < MAX_NON_RELAY_PATHS
        assert_eq!(35, paths.len());
    }

    #[test]
    fn test_prune_preserves_never_dialed() {
        let mut paths = FxHashMap::default();

        // Add 20 never-dialed paths (PathStatus::Unknown)
        for i in 0..20 {
            paths.insert(ip_addr(i), PathState::default());
        }

        // Add 15 failed paths to trigger pruning
        for i in 20..35 {
            paths.insert(ip_addr(i), path_state_unusable());
        }

        prune_non_relay_paths(&mut paths);

        // Never-dialed paths should be preserved
        for i in 0..20 {
            assert!(paths.contains_key(&ip_addr(i)));
        }
    }

    #[test]
    fn test_prune_all_paths_failed() {
        let mut paths = FxHashMap::default();

        // Add 40 failed holepunch paths (all paths have failed)
        for i in 0..40 {
            paths.insert(ip_addr(i), path_state_unusable());
        }

        assert_eq!(40, paths.len());
        prune_non_relay_paths(&mut paths);

        // Should keep MAX_NON_RELAY_PATHS instead of pruning everything
        // This prevents catastrophic loss of all path information
        assert_eq!(
            MAX_NON_RELAY_PATHS,
            paths.len(),
            "should keep MAX_NON_RELAY_PATHS when all paths failed"
        );
    }

    #[test]
    fn test_insert_open_path() {
        let mut state = RemotePathState::new(Default::default());
        let addr = ip_addr(1000);
        let source = Source::Connection;

        assert!(state.is_empty());

        state.insert_open_path(addr.clone(), source.clone());

        assert!(!state.is_empty());
        assert!(state.paths.contains_key(&addr));
        let path = &state.paths[&addr];
        assert!(matches!(path.status, PathStatus::Open));
        assert_eq!(path.sources.len(), 1);
        assert!(path.sources.contains_key(&source));
    }

    #[test]
    fn test_abandoned_path() {
        let metrics = Arc::new(SocketMetrics::default());
        let mut state = RemotePathState::new(metrics.clone());

        // Test: Open goes to Inactive
        let addr_open = ip_addr(1000);
        state.insert_open_path(addr_open.clone(), Source::Connection);
        assert!(matches!(state.paths[&addr_open].status, PathStatus::Open));
        assert_eq!(metrics.transport_ip_paths_added.get(), 1);

        state.abandoned_path(&addr_open);
        assert!(matches!(
            state.paths[&addr_open].status,
            PathStatus::Inactive(_)
        ));
        assert_eq!(metrics.transport_ip_paths_added.get(), 1);
        assert_eq!(metrics.transport_ip_paths_removed.get(), 1);

        // Test: Inactive stays Inactive
        state.abandoned_path(&addr_open);
        assert!(matches!(
            state.paths[&addr_open].status,
            PathStatus::Inactive(_)
        ));
        assert_eq!(metrics.transport_ip_paths_added.get(), 1);
        assert_eq!(metrics.transport_ip_paths_removed.get(), 1);

        // Test: Unknown goes to Unusable
        let addr_unknown = ip_addr(2000);
        state.insert_multiple([addr_unknown.clone()].into_iter(), Source::Connection);
        assert!(matches!(
            state.paths[&addr_unknown].status,
            PathStatus::Unknown
        ));
        assert_eq!(metrics.transport_ip_paths_added.get(), 1);
        assert_eq!(metrics.transport_ip_paths_removed.get(), 1);

        state.abandoned_path(&addr_unknown);
        assert!(matches!(
            state.paths[&addr_unknown].status,
            PathStatus::Unusable
        ));
        assert_eq!(metrics.transport_ip_paths_added.get(), 1);
        assert_eq!(metrics.transport_ip_paths_removed.get(), 1);

        // Test: Unusable stays Unusable
        state.abandoned_path(&addr_unknown);
        assert!(matches!(
            state.paths[&addr_unknown].status,
            PathStatus::Unusable
        ));
        assert_eq!(metrics.transport_ip_paths_added.get(), 1);
        assert_eq!(metrics.transport_ip_paths_removed.get(), 1);

        // Test: Unusable can go to open
        state.insert_open_path(addr_unknown.clone(), Source::Connection);
        assert!(matches!(
            state.paths[&addr_unknown].status,
            PathStatus::Open
        ));
        assert_eq!(metrics.transport_ip_paths_added.get(), 2);
        assert_eq!(metrics.transport_ip_paths_removed.get(), 1);
    }

    /// An empty `insert_multiple` must not drain pending resolve requests.
    ///
    /// This reproduces the race where multiple concurrent `connect_with_opts`
    /// calls send `ResolveRemote` messages with empty addrs. The first pushes
    /// a tx, then the second's `insert_multiple([])` used to drain that tx
    /// with `NoResults { errors: [] }`, even though an address lookup was
    /// still in flight and would shortly have resolved it.
    #[test]
    fn empty_insert_does_not_drain_pending() {
        let metrics = Arc::new(SocketMetrics::default());
        let mut state = RemotePathState::new(metrics);

        let (tx, mut rx) = oneshot::channel();
        state.resolve_remote(tx);

        // Second concurrent resolve arrives with empty addrs (no app-provided
        // addresses) while address lookup is still running.
        state.insert_multiple(std::iter::empty(), Source::App);

        assert!(
            rx.try_recv().is_err(),
            "pending tx must stay pending while paths are empty and lookup is in flight"
        );

        // When real addresses arrive, the tx resolves Ok.
        state.insert_multiple([ip_addr(4242)].into_iter(), Source::App);
        let resolved = rx.try_recv().expect("tx should have been woken");
        assert!(resolved.is_ok(), "expected Ok once a path was added");
    }

    /// `address_lookup_finished(Ok(()))` drains pending requests with `NoResults` when no paths are known.
    ///
    /// This is the "lookup done but nothing was found" signal and it must
    /// still reach callers.
    #[test]
    fn address_lookup_finished_empty_emits_no_results() {
        let metrics = Arc::new(SocketMetrics::default());
        let mut state = RemotePathState::new(metrics);

        let (tx, mut rx) = oneshot::channel();
        state.resolve_remote(tx);

        state.address_lookup_finished(Ok(()));

        let resolved = rx.try_recv().expect("tx should have been woken");
        assert!(matches!(
            resolved,
            Err(AddressLookupFailed::NoResults { .. })
        ));
    }

    // ---------------------------------------------------------------------------------
    // HYPER PATCH: adaptive path lifecycle.
    //
    // These exercise `retire_idle_non_relay_paths` directly, with explicit `now` /
    // `idle_after` / `spares`, so they never touch the process-global flag — the test
    // binary is multi-threaded and flipping a global would race the default-off assertions
    // in `crate::radio`.
    // ---------------------------------------------------------------------------------

    /// The defect this whole change exists for: idle paths accumulate and are never
    /// retired, because upstream only prunes under cap pressure that never arrives.
    ///
    /// Ten long-dead paths, nowhere near `MAX_NON_RELAY_PATHS`, so the upstream prune is a
    /// no-op — this is the shape the on-device census actually showed.
    #[test]
    fn lifecycle_retires_long_idle_paths_keeping_spares() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..10 {
            paths.insert(ip_addr(i), path_state_inactive(long_ago(now, i as u64)));
        }

        // Upstream prune does nothing at all here: that is the bug.
        prune_non_relay_paths(&mut paths);
        assert_eq!(10, paths.len(), "cap-pressure pruning cannot help here");

        let retired = retire(&mut paths, now);
        assert_eq!(10 - crate::radio::PATH_RETIRE_SPARES, retired);
        assert_eq!(crate::radio::PATH_RETIRE_SPARES, paths.len());
    }

    /// The spares kept must be the freshest knowledge we have, not an arbitrary subset.
    #[test]
    fn lifecycle_keeps_the_freshest_spares() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        // Port i was abandoned `i` seconds further in the past, so port 0 is the freshest.
        for i in 0..8 {
            paths.insert(ip_addr(i), path_state_inactive(long_ago(now, i as u64)));
        }

        retire(&mut paths, now);

        assert_eq!(crate::radio::PATH_RETIRE_SPARES, paths.len());
        for i in 0..crate::radio::PATH_RETIRE_SPARES as u16 {
            assert!(
                paths.contains_key(&ip_addr(i)),
                "port {i} is among the freshest and must be kept as a spare"
            );
        }
    }

    /// **The relay is the delivery backstop — every gossip broadcast rides it.** Retiring
    /// it would silently lose messages, so it is never eligible however old it looks.
    #[test]
    fn lifecycle_never_retires_the_relay_path() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        let relay = relay_addr(7);
        // Even marked long-abandoned, the relay must survive.
        paths.insert(relay.clone(), path_state_inactive(long_ago(now, 9_999)));
        for i in 0..10 {
            paths.insert(ip_addr(i), path_state_inactive(long_ago(now, i as u64)));
        }

        retire(&mut paths, now);

        assert!(
            paths.contains_key(&relay),
            "the relay path must never be retired"
        );
    }

    /// A path that is live on a connection is untouchable whatever its recorded status
    /// says. The status genuinely can lag: closing a whole connection does not mark its
    /// paths abandoned, so trusting it alone would eventually retire a working path.
    #[test]
    fn lifecycle_never_retires_a_path_in_use() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..10 {
            paths.insert(ip_addr(i), path_state_inactive(long_ago(now, i as u64)));
        }

        // Port 9 is the oldest, so it would be first to go — but it is carrying traffic.
        let live = ip_addr(9);
        let in_use = HashSet::from_iter([live.clone()]);
        retire_idle_non_relay_paths(
            &mut paths,
            &in_use,
            now,
            crate::radio::PATH_RETIRE_IDLE_AFTER,
            0,
        );

        assert!(
            paths.contains_key(&live),
            "a path live on a connection must never be retired"
        );
        assert_eq!(1, paths.len(), "everything else was retirable");
    }

    /// Open paths and untried candidates are never retired on idleness: the first is
    /// carrying traffic, the second is a dial hint we have simply not got to yet.
    #[test]
    fn lifecycle_leaves_open_and_untried_paths_alone() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..5 {
            paths.insert(ip_addr(i), PathState::default()); // Unknown: never dialled
        }
        for i in 5..10 {
            let mut st = PathState::default();
            st.status = PathStatus::Open;
            paths.insert(ip_addr(i), st);
        }

        let retired = retire(&mut paths, now);

        assert_eq!(0, retired);
        assert_eq!(10, paths.len(), "nothing here has stopped working");
    }

    /// Only paths idle for longer than the threshold are eligible. A path abandoned a
    /// moment ago is exactly the one a reconnect is about to want.
    #[test]
    fn lifecycle_does_not_retire_recently_idle_paths() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..10 {
            // One second short of the threshold.
            let since = now - crate::radio::PATH_RETIRE_IDLE_AFTER + Duration::from_secs(1);
            paths.insert(ip_addr(i), path_state_inactive(since));
        }

        assert_eq!(0, retire(&mut paths, now));
        assert_eq!(10, paths.len());
    }

    /// A path that was proven unusable (holepunching tried and failed) ages out on the
    /// stamp recorded at that moment.
    #[test]
    fn lifecycle_retires_proven_unusable_paths() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..10 {
            paths.insert(
                ip_addr(i),
                path_state_unusable_since(long_ago(now, i as u64)),
            );
        }

        let retired = retire(&mut paths, now);

        assert_eq!(10 - crate::radio::PATH_RETIRE_SPARES, retired);
    }

    /// An `Unusable` path carrying no stamp is upstream-shaped state we cannot date, so it
    /// is left to the cap-pressure prune rather than retired on a guess.
    #[test]
    fn lifecycle_ignores_unusable_paths_with_no_timestamp() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..10 {
            paths.insert(ip_addr(i), path_state_unusable());
        }

        assert_eq!(0, retire(&mut paths, now));
        assert_eq!(10, paths.len());
    }

    /// **Never prune the last usable path.** Even with the spare floor set to zero — which
    /// the shipped constants forbid — the table is never emptied.
    #[test]
    fn lifecycle_never_empties_the_path_table() {
        let now = Instant::now();
        let mut paths = FxHashMap::default();
        for i in 0..3 {
            paths.insert(ip_addr(i), path_state_inactive(long_ago(now, i as u64)));
        }

        let retired = retire_idle_non_relay_paths(
            &mut paths,
            &HashSet::new(),
            now,
            crate::radio::PATH_RETIRE_IDLE_AFTER,
            0,
        );

        assert_eq!(0, retired, "emptying the table must be refused outright");
        assert_eq!(3, paths.len());
    }

    /// The flag gate. Default-off must be observable through the real entry point, not
    /// just asserted of the constant.
    #[test]
    fn lifecycle_is_off_by_default() {
        let mut state = RemotePathState::new(Default::default());
        let addr = ip_addr(1000);
        state.insert_open_path(addr.clone(), Source::Connection);
        state.abandoned_path(&addr);

        assert_eq!(
            0,
            state.retire_idle_paths(&HashSet::new()),
            "the lifecycle must be inert until the flag is set"
        );
        assert_eq!(1, state.len());
    }

    /// Reproduction of the census actually measured on hardware, and what this change does
    /// to it.
    ///
    /// The OnePlus 6 reported, unchanged in every 30s sample across a six-minute idle
    /// window on Wi-Fi:
    ///
    /// ```text
    /// carrier pathcensus: peer=bc5ee96a ip_active=1 ip_inactive=4 relay_active=1 \
    ///                     relay_inactive=0 other=0 total=6
    /// ```
    ///
    /// One working direct path, one relay, and **four dead IP paths held forever** — the
    /// table never shrank, because nothing in iroh retires a path on idleness. This test
    /// builds that exact shape and asserts what the lifecycle converges it to: the working
    /// path and the relay both untouched, the dead four cut back to the spare allowance.
    #[test]
    fn measured_device_census_converges() {
        let now = Instant::now();
        let mut state = RemotePathState::new(Default::default());

        // ip_active=1: the path actually carrying traffic.
        let live = ip_addr(41641);
        state.insert_open_path(live.clone(), Source::Connection);
        // relay_active=1: the delivery backstop.
        let relay = relay_addr(11);
        state.insert_open_path(relay.clone(), Source::Connection);
        // ip_inactive=4: long-dead paths nothing will ever retire upstream.
        for i in 0..4 {
            state
                .paths
                .insert(ip_addr(i), path_state_inactive(long_ago(now, i as u64)));
        }

        assert_eq!((1, 4, 1, 0), census(&state), "the measured before-state");

        // Upstream's only pruning is cap-driven, and 6 paths is nowhere near the cap of
        // 30 — so it is a no-op, which is precisely why the device never recovers.
        state.prune_paths();
        assert_eq!(
            (1, 4, 1, 0),
            census(&state),
            "cap-pressure pruning cannot retire anything here: this is the defect"
        );

        // The lifecycle sweep, with the path that is carrying traffic declared in use.
        let in_use = HashSet::from_iter([live.clone(), relay.clone()]);
        retire_idle_non_relay_paths(
            &mut state.paths,
            &in_use,
            now,
            crate::radio::PATH_RETIRE_IDLE_AFTER,
            crate::radio::PATH_RETIRE_SPARES,
        );

        assert_eq!(
            (1, crate::radio::PATH_RETIRE_SPARES, 1, 0),
            census(&state),
            "converges on the working path, the relay, and a small spare"
        );
        assert!(
            state.paths.contains_key(&live),
            "the working path must survive"
        );
        assert!(
            state.paths.contains_key(&relay),
            "the relay must survive: it is the delivery backstop"
        );
    }

    /// The new state transition: being proven unusable stamps a clock, and coming back
    /// into service clears it. Without the clear, a path that recovered would still be
    /// retired on the strength of a failure it has since disproved.
    #[test]
    fn unusable_since_is_stamped_and_cleared() {
        let mut state = RemotePathState::new(Default::default());
        let addr = ip_addr(1234);

        // Never dialled -> proven unusable: stamped.
        state.insert_multiple([addr.clone()].into_iter(), Source::App);
        assert!(state.paths[&addr].unusable_since.is_none());
        state.abandoned_path(&addr);
        let stamped = state.paths[&addr]
            .unusable_since
            .expect("proving a path unusable must stamp it");

        // Repeat failures must not refresh it: the age measures time since we proved it
        // dead, not time since we last retried something we already knew was dead.
        state.abandoned_path(&addr);
        assert_eq!(Some(stamped), state.paths[&addr].unusable_since);

        // Back in service: the stamp is history.
        state.insert_open_path(addr.clone(), Source::Connection);
        assert!(
            state.paths[&addr].unusable_since.is_none(),
            "a path that came back must not carry its old failure"
        );
    }

    // ---------------------------------------------------------------------------------
    // HYPER PATCH: the speculative fan-out budget.
    //
    // Every test below drives the parameterised entry points, never the process-global
    // flag: the test binary is multi-threaded and flipping a global would race
    // `fanout_is_off_by_default` and every other default-off assertion in this crate.
    // ---------------------------------------------------------------------------------

    /// The shipped limits, so the tests measure what the device will run.
    const BUDGET: u32 = crate::radio::FANOUT_PROBE_BUDGET;
    const COLD: usize = crate::radio::FANOUT_COLD_RETRIES;
    const COLD_NO_RELAY: usize = crate::radio::FANOUT_COLD_RETRIES_NO_RELAY;

    /// A saturated path table exactly as measured on the handset: `MAX_NON_RELAY_PATHS`
    /// private candidates that will never answer, plus the relay that carries everything.
    fn saturated_table() -> (RemotePathState, transports::Addr) {
        let mut state = RemotePathState::new(Default::default());
        let relay = relay_addr(1);
        state.insert_multiple([relay.clone()].into_iter(), Source::App);
        state.insert_multiple(
            (0..MAX_NON_RELAY_PATHS as u16).map(|i| ip_addr(40000 + i)),
            Source::App,
        );
        assert_eq!(MAX_NON_RELAY_PATHS + 1, state.paths.len());
        (state, relay)
    }

    /// One QUIC connect: the Initial plus its PTO retransmits, which is what upstream
    /// multiplies by the size of the path table. Returns the total destination-sends.
    fn one_connect_ladder(state: &mut RemotePathState) -> usize {
        (0..BUDGET)
            .map(|_| state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY).len())
            .sum()
    }

    #[test]
    fn fanout_is_off_by_default() {
        // With the switch unarmed the send path must see the whole table, exactly as
        // upstream, however many times it is called.
        let (mut state, _relay) = saturated_table();
        for _ in 0..50 {
            assert_eq!(
                MAX_NON_RELAY_PATHS + 1,
                state.fanout_addrs().len(),
                "the flag is off; nothing may be withheld"
            );
        }
        assert_eq!(0, state.rearm_fanout(), "re-arm is a no-op with the flag off");
    }

    #[test]
    fn fanout_stops_paying_for_candidates_that_never_answer() {
        // The measured defect and its fix, in one test. First connect: everything gets a
        // full, fair trial, so the cost is exactly upstream's. Second connect: the dead
        // candidates have spent their budget and the burst collapses to the relay plus the
        // rotating re-probe window.
        let (mut state, _relay) = saturated_table();

        let first = one_connect_ladder(&mut state);
        assert_eq!(
            (MAX_NON_RELAY_PATHS + 1) * BUDGET as usize,
            first,
            "first contact must be untouched: every candidate gets one full ladder"
        );

        let second = one_connect_ladder(&mut state);
        assert_eq!(
            (1 + COLD) * BUDGET as usize,
            second,
            "steady state is the relay plus the re-probe window, not the whole table"
        );

        // The number that matters. 30 dead candidates measured 3.649 pkt/s at idle; this
        // is the fraction of that which survives.
        assert!(
            second * 8 < first,
            "the second connect must cost less than an eighth of the first, \
             not merely a little less: got {second} vs {first}"
        );
    }

    #[test]
    fn fanout_cost_stops_scaling_with_dead_candidates() {
        // "More contacts must mean LESS traffic per contact." The steady-state cost of a
        // connect must be independent of how many dead candidates we are carrying — three
        // or thirty, the burst is the same size.
        let cost = |n: u16| {
            let mut state = RemotePathState::new(Default::default());
            state.insert_multiple([relay_addr(1)].into_iter(), Source::App);
            state.insert_multiple((0..n).map(|i| ip_addr(40000 + i)), Source::App);
            one_connect_ladder(&mut state); // spend the trial
            one_connect_ladder(&mut state) // steady state
        };
        assert_eq!(
            cost(3),
            cost(MAX_NON_RELAY_PATHS as u16),
            "steady-state burst size must not scale with the dead-candidate count"
        );
    }

    #[test]
    fn fanout_never_withholds_a_relay_or_custom_address() {
        // The delivery backstop. The relay leg is what every first contact with a NAT'd
        // peer and every gossip broadcast actually rides, so it is never budgeted, never
        // rotated and never counted against any window — however cold everything is.
        let (mut state, relay) = saturated_table();
        for round in 0..200 {
            let chosen = state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY);
            assert!(
                chosen.contains(&relay),
                "round {round}: the relay must be in every speculative burst"
            );
        }
    }

    #[test]
    fn fanout_re_probes_every_cold_candidate_within_a_bounded_number_of_rounds() {
        // The rotation guarantee, and the reason the window is taken from a SORTED list
        // rather than from `FxHashMap` order: a fixed-size slice of an arbitrarily-ordered
        // map could exclude the one live candidate on every send for the life of the
        // process, turning a slow connect into a permanent failure.
        let (mut state, _relay) = saturated_table();
        one_connect_ladder(&mut state); // everything goes cold

        let bound = MAX_NON_RELAY_PATHS.div_ceil(COLD);
        let mut seen: HashSet<transports::Addr> = HashSet::new();
        for _ in 0..bound {
            seen.extend(
                state
                    .budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY)
                    .into_iter()
                    .filter(|a| a.is_ip()),
            );
        }
        assert_eq!(
            MAX_NON_RELAY_PATHS,
            seen.len(),
            "every cold candidate must be re-probed within ceil(n/window) rounds"
        );
    }

    #[test]
    fn fanout_rotation_is_independent_of_map_insertion_order() {
        // Same set of candidates, opposite insertion order. `FxHashMap` iteration order
        // differs between the two; the re-probe sequence must not.
        let build = |reverse: bool| {
            let mut state = RemotePathState::new(Default::default());
            state.insert_multiple([relay_addr(1)].into_iter(), Source::App);
            let mut ports: Vec<u16> = (0..MAX_NON_RELAY_PATHS as u16).map(|i| 40000 + i).collect();
            if reverse {
                ports.reverse();
            }
            state.insert_multiple(ports.into_iter().map(ip_addr), Source::App);
            one_connect_ladder(&mut state);
            (0..12)
                .map(|_| {
                    let mut round: Vec<transports::Addr> = state
                        .budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY)
                        .into_iter()
                        .filter(|a| a.is_ip())
                        .collect();
                    round.sort();
                    round
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            build(false),
            build(true),
            "the re-probe sequence must be a deterministic sweep, not a lottery over map order"
        );
    }

    #[test]
    fn fanout_never_goes_silent_on_a_remote_with_no_backstop() {
        // The delivery-critical case: an mDNS-discovered LAN peer, or a ticket whose relay
        // was filtered out, has no relay in its table at all. This loop is then the ONLY
        // way a packet ever reaches it, so it must never be narrowed to nothing.
        let mut state = RemotePathState::new(Default::default());
        state.insert_multiple(
            (0..MAX_NON_RELAY_PATHS as u16).map(|i| ip_addr(40000 + i)),
            Source::App,
        );
        for round in 0..300 {
            let chosen = state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY);
            assert!(
                !chosen.is_empty(),
                "round {round}: a remote with no relay must always have somewhere to send"
            );
        }
    }

    #[test]
    fn fanout_widens_when_there_is_no_backstop() {
        // With no relay, first contact must not get slower: one connect ladder has to be
        // able to sweep a fully saturated table, so the window widens instead of narrowing.
        let mut state = RemotePathState::new(Default::default());
        state.insert_multiple(
            (0..MAX_NON_RELAY_PATHS as u16).map(|i| ip_addr(40000 + i)),
            Source::App,
        );
        one_connect_ladder(&mut state); // everything goes cold

        let mut seen: HashSet<transports::Addr> = HashSet::new();
        for _ in 0..BUDGET {
            seen.extend(state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY));
        }
        assert_eq!(
            MAX_NON_RELAY_PATHS,
            seen.len(),
            "with no relay, a single ladder must still reach every candidate"
        );
    }

    #[test]
    fn fanout_no_relay_window_covers_the_cap() {
        // Pins `radio`'s restated copy of MAX_NON_RELAY_PATHS to the real one, so the two
        // cannot drift apart silently.
        assert!(BUDGET as usize * COLD_NO_RELAY >= MAX_NON_RELAY_PATHS);
    }

    #[test]
    fn fanout_re_arms_on_proof_of_life() {
        // Per-address re-arm. A path opening on an address is the strongest evidence there
        // is that datagrams to it are worth paying for, so it gets its full budget back —
        // otherwise a peer that came back would be stuck in the re-probe window.
        let (mut state, _relay) = saturated_table();
        let live = ip_addr(40007);
        one_connect_ladder(&mut state);
        assert_eq!(BUDGET, state.paths[&live].fanout_probes);

        state.insert_open_path(live.clone(), Source::Connection);
        assert_eq!(
            0, state.paths[&live].fanout_probes,
            "a path that came up must not still be serving a budget sentence"
        );
        assert!(
            state
                .budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY)
                .contains(&live),
            "and it must be back in the very next burst"
        );
    }

    #[test]
    fn fanout_re_arms_the_whole_table_on_a_link_change() {
        // Whole-table re-arm. The addresses we proved silent were proved silent on a
        // network we are no longer on, so a link change hands every one of them its full
        // trial back — the burst returns to upstream's width until they re-prove
        // themselves.
        let (mut state, _relay) = saturated_table();
        let full = one_connect_ladder(&mut state);
        let narrowed = one_connect_ladder(&mut state);
        assert!(narrowed < full, "sanity: it did narrow");

        assert_eq!(
            MAX_NON_RELAY_PATHS,
            state.rearm_fanout_now(),
            "every cold candidate must be re-armed"
        );
        assert_eq!(
            (MAX_NON_RELAY_PATHS + 1) * BUDGET as usize,
            one_connect_ladder(&mut state),
            "after a link change the fan-out is upstream-wide again"
        );
    }

    #[test]
    fn fanout_does_not_re_arm_on_a_re_advertised_address() {
        // The pkarr publisher republishes every five minutes and address lookup re-inserts
        // what it finds. If a re-advertisement counted as news, the budget would be handed
        // back on a schedule and this whole switch would quietly do nothing.
        let (mut state, _relay) = saturated_table();
        one_connect_ladder(&mut state);
        let narrowed = one_connect_ladder(&mut state);

        state.insert_multiple(
            (0..MAX_NON_RELAY_PATHS as u16).map(|i| ip_addr(40000 + i)),
            Source::App,
        );
        assert_eq!(
            narrowed,
            one_connect_ladder(&mut state),
            "re-learning an address we already knew is not evidence it works"
        );
    }

    #[test]
    fn fanout_gives_a_genuinely_new_candidate_a_full_trial() {
        // The other half of the same rule: a candidate we have never seen before arrives
        // as a fresh entry and is dialled exactly as upstream would dial it. This is what
        // keeps a peer that moves onto our LAN reachable without waiting for a link change.
        let (mut state, _relay) = saturated_table();
        one_connect_ladder(&mut state);

        let fresh = ip_addr(51234);
        state.insert_multiple([fresh.clone()].into_iter(), Source::App);
        for round in 0..BUDGET {
            assert!(
                state
                    .budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY)
                    .contains(&fresh),
                "round {round}: a new candidate must get its whole ladder"
            );
        }
    }

    #[test]
    fn fanout_charges_only_what_it_sends_to() {
        // A candidate we declined to probe has not had its trial, so it must not lose any
        // of it. Getting this wrong would silently retire the table in one burst instead
        // of giving each candidate a full ladder.
        let (mut state, _relay) = saturated_table();
        for _ in 0..BUDGET {
            state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY);
        }
        let before: Vec<u32> = {
            let mut v: Vec<_> = state
                .paths
                .iter()
                .filter(|(a, _)| a.is_ip())
                .map(|(_, s)| s.fanout_probes)
                .collect();
            v.sort_unstable();
            v
        };
        assert!(
            before.iter().all(|&p| p == BUDGET),
            "every candidate should have spent exactly its ladder, got {before:?}"
        );

        let chosen = state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY);
        for (addr, path) in state.paths.iter().filter(|(a, _)| a.is_ip()) {
            let expected = if chosen.contains(addr) {
                BUDGET + 1
            } else {
                BUDGET
            };
            assert_eq!(
                expected, path.fanout_probes,
                "{addr:?} was charged for a datagram it never received"
            );
        }
    }

    /// The measured device case, end to end: the census that four audits chased.
    ///
    /// 30 private candidates to one peer, one relay, nothing reachable. Upstream pays
    /// `31 x 8` destination-sends per connect round; with the budget armed it pays
    /// `3 x 8` — and the relay, the thing that actually delivers, is in every single one.
    #[test]
    fn measured_fanout_burst_collapses() {
        let (mut state, relay) = saturated_table();

        let upstream_round = (MAX_NON_RELAY_PATHS + 1) * BUDGET as usize;
        assert_eq!(upstream_round, one_connect_ladder(&mut state));

        let mut rounds = Vec::new();
        for _ in 0..10 {
            let mut sends = 0;
            for _ in 0..BUDGET {
                let chosen = state.budgeted_fanout_addrs(BUDGET, COLD, COLD_NO_RELAY);
                assert!(chosen.contains(&relay), "the relay is never withheld");
                sends += chosen.len();
            }
            rounds.push(sends);
        }

        let steady = (1 + COLD) * BUDGET as usize;
        assert!(
            rounds.iter().all(|&r| r == steady),
            "every steady-state round must cost {steady}, got {rounds:?}"
        );
        // 248 -> 24 destination-sends per connect round.
        assert!(
            steady * 10 <= upstream_round,
            "the burst must collapse by at least 10x: {steady} vs {upstream_round}"
        );
    }
}
