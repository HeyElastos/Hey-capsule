use std::{
    collections::{BTreeSet, VecDeque},
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::Poll,
};

use iroh_base::{CustomAddr, EndpointId, RelayUrl, TransportAddr};
use n0_error::StackResultExt;
use n0_future::{
    FuturesUnordered, MaybeFuture, MergeUnbounded, Stream, StreamExt,
    boxed::BoxStream,
    task::JoinSet,
    time::{self, Duration, Instant},
};
use n0_watcher::Watcher;
use noq::{Closed, PathStats, PathStatus, WeakConnectionHandle};
use noq_proto::{PathError, PathEvent as NoqPathEvent, PathId, n0_nat_traversal};
use rustc_hash::FxHashMap;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, Level, Span, debug, error, event, info_span, instrument, trace, warn};

use self::path_state::RemotePathState;
pub(crate) use self::path_watcher::PathStateReceiver;
pub use self::{
    path_watcher::{Path, PathEvent, PathEventStream, PathList, PathListIter, PathListStream},
    remote_info::{RemoteInfo, TransportAddrInfo, TransportAddrUsage},
};
use super::Source;
use crate::{
    address_lookup::{AddressLookupFailed, AddressLookupServices, Item as AddressLookupItem},
    endpoint::DirectAddr,
    socket::{
        Metrics as SocketMetrics, RELAY_PATH_MAX_IDLE_TIMEOUT,
        mapped_addrs::{AddrMap, CustomMappedAddr, RelayMappedAddr},
        remote_map::remote_state::path_watcher::PathStateSender,
        transports::{self, OwnedTransmit, TransportsSender},
    },
};

mod path_state;
mod path_watcher;
mod remote_info;

/// How often to attempt holepunching.
///
/// If there have been no changes to the NAT address candidates, holepunching will not be
/// attempted more frequently than at this interval.
const HOLEPUNCH_ATTEMPTS_INTERVAL: Duration = Duration::from_secs(5);

/// The latency at or under which we don't try to upgrade to a better path.
const GOOD_ENOUGH_LATENCY: Duration = Duration::from_millis(10);

// TODO: use this
// /// How long since the last activity we try to keep an established endpoint peering alive.
// ///
// /// It's also the idle time at which we stop doing QAD queries to keep NAT mappings alive.
// pub(super) const SESSION_ACTIVE_TIMEOUT: Duration = Duration::from_secs(45);

/// How often we try to upgrade to a better path.
///
/// Even if we have some non-relay route that works.
const UPGRADE_INTERVAL: Duration = Duration::from_secs(60);

/// The time after which an idle [`RemoteStateActor`] stops.
///
/// The actor only enters the idle state if no connections are active and no inbox senders exist
/// apart from the one stored in the endpoint map. Stopping and restarting the actor in this state
/// is not an issue; a timeout here serves the purpose of not stopping-and-recreating actors
/// in a high frequency, and to keep data about previous path around for subsequent connections.
const ACTOR_MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// A stream of events from all paths for all connections.
///
/// The connection is identified using [`ConnId`].  The event `Err` variant happens when the
/// actor has lagged processing the events, which is rather critical for us.
type PathEvents = MergeUnbounded<
    Pin<Box<dyn Stream<Item = (ConnId, Result<NoqPathEvent, noq::Lagged>)> + Send + Sync>>,
>;

/// A stream of events of announced NAT traversal candidate addresses for all connections.
///
/// The connection is identified using [`ConnId`].
type AddrEvents = MergeUnbounded<
    Pin<
        Box<
            dyn Stream<Item = (ConnId, Result<n0_nat_traversal::Event, noq::Lagged>)> + Send + Sync,
        >,
    >,
>;

/// The state we need to know about a single remote endpoint.
///
/// This actor manages all connections to the remote endpoint.  It will trigger holepunching
/// and select the best path etc.
pub(super) struct RemoteStateActor {
    /// All connections we have to this remote endpoint.
    connections: FxHashMap<ConnId, ConnectionState>,
    /// State of the actor and hooks into the rest of the remote endpoint.
    ///
    /// This is on a separate struct so that we can have parallel mutable borrows to `connections` and `state`.
    state: State,
}

/// State of the [`RemoteStateActor`] and hooks into the rest of the remote endpoint.
struct State {
    /// The endpoint ID of the remote endpoint.
    endpoint_id: EndpointId,

    // Hooks into the rest of the Socket.
    //
    /// Metrics.
    metrics: Arc<SocketMetrics>,
    /// Our local addresses.
    ///
    /// These are our local addresses and any reflexive transport addresses.
    local_direct_addrs: n0_watcher::Direct<BTreeSet<DirectAddr>>,
    /// The mapping between endpoints via a relay and their [`RelayMappedAddr`]s.
    relay_mapped_addrs: AddrMap<(RelayUrl, EndpointId), RelayMappedAddr>,
    /// The mapping between custom transport addresses and their [`CustomMappedAddr`]s.
    custom_mapped_addrs: AddrMap<CustomAddr, CustomMappedAddr>,
    /// Address lookup service, cloned from the socket.
    address_lookup: AddressLookupServices,

    // Internal state - Noq Connections we are managing.
    //
    /// Notifications when connections are closed.
    connections_close: FuturesUnordered<OnClosed>,
    /// Events emitted by Noq about path changes, for all paths, all connections.
    path_events: PathEvents,
    /// A stream of events of announced NAT traversal candidate addresses for all connections.
    addr_events: AddrEvents,

    // Internal state - Holepunching and path state.
    //
    /// All possible paths we are aware of.
    ///
    /// These paths might be entirely impossible to use, since they are added by Address Lookup
    /// mechanisms.  The are only potentially usable.
    paths: RemotePathState,
    /// Information about the last holepunching attempt.
    last_holepunch: Option<HolepunchAttempt>,
    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): consecutive holepunch
    /// rounds that ran with an UNCHANGED candidate set, i.e. blind retries.
    ///
    /// Upstream re-punches whenever `check_connections` is unhappy, and it is unhappy
    /// forever on cellular: it only accepts `min_ip_rtt <= GOOD_ENOUGH_LATENCY` (10ms),
    /// which LTE (25-60ms) can never satisfy. `HOLEPUNCH_ATTEMPTS_INTERVAL` does not save
    /// us — it is a 5s *floor*, long expired by the time the 60s `UPGRADE_INTERVAL` comes
    /// round — so we mint a fresh connection and a fresh set of paths every 60s forever.
    /// That is the churn behind the steady idle radio traffic, and it is what pins the path
    /// census at the `MAX_MULTIPATH_PATHS` cap.
    ///
    /// This counter drives an exponential backoff for that specific case, and is reset to
    /// zero by every event that could actually change the outcome.
    holepunch_idle_rounds: u32,
    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): while `Some(t)` and
    /// `t` is in the future, the adaptive path-lifecycle sweep stands down.
    ///
    /// This is the re-widening half of the lifecycle. Narrowing onto "the working path plus
    /// a small spare" is only safe if we widen back out the moment the ground moves, and
    /// **that must be driven by a real signal, never by a blind timer**. So nothing sets
    /// this except the four events that genuinely invalidate what we know about
    /// reachability — a link change, a new candidate address, a direct path coming up, and
    /// losing the last direct path — via [`RemoteStateActor::arm_path_widening`].
    ///
    /// Holding the *sweep* off rather than actively re-adding paths is deliberate: the
    /// machinery that repopulates the table (holepunching, address lookup, QNT candidate
    /// exchange) already runs on those same signals. All this has to do is stop deleting
    /// its results while it works.
    lifecycle_hold_until: Option<Instant>,

    /// The path we currently consider the preferred path to the remote endpoint.
    ///
    /// **We expect this path to work.** If we become aware this path is broken then it is
    /// set back to `None`.  Having a selected path does not mean we may not be able to get
    /// a better path: e.g. when the selected path is a relay path we still need to trigger
    /// holepunching regularly.
    ///
    /// We only select a path once the path is functional in Noq.
    selected_path: Option<transports::FourTuple>,
    /// Time at which we should schedule the next holepunch attempt.
    scheduled_holepunch: Option<Instant>,
    /// When to next attempt opening paths in [`Self::pending_open_paths`].
    scheduled_open_path: Option<Instant>,
    /// Paths which we still need to open.
    ///
    /// They failed to open because we did not have enough CIDs issued by the remote.
    pending_open_paths: VecDeque<transports::FourTuple>,

    // Internal state - address lookup
    //
    /// Stream of Address Lookup results, or always pending if Address Lookup is not running.
    address_lookup_stream: Option<BoxStream<Result<AddressLookupItem, AddressLookupFailed>>>,

    /// The path selector used to pick the preferred path among the candidates.
    path_selector: Arc<dyn PathSelector>,
}

impl RemoteStateActor {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        endpoint_id: EndpointId,
        local_direct_addrs: n0_watcher::Direct<BTreeSet<DirectAddr>>,
        relay_mapped_addrs: AddrMap<(RelayUrl, EndpointId), RelayMappedAddr>,
        custom_mapped_addrs: AddrMap<CustomAddr, CustomMappedAddr>,
        metrics: Arc<SocketMetrics>,
        address_lookup: AddressLookupServices,
        path_selector: Arc<dyn PathSelector>,
    ) -> Self {
        Self {
            connections: FxHashMap::default(),
            state: State {
                endpoint_id,
                metrics: metrics.clone(),
                local_direct_addrs,
                relay_mapped_addrs,
                custom_mapped_addrs,
                address_lookup,
                connections_close: Default::default(),
                path_events: Default::default(),
                addr_events: Default::default(),
                paths: RemotePathState::new(metrics),
                last_holepunch: None,
                holepunch_idle_rounds: 0,
                lifecycle_hold_until: None,
                selected_path: Default::default(),
                scheduled_holepunch: None,
                scheduled_open_path: None,
                pending_open_paths: VecDeque::new(),
                address_lookup_stream: None,
                path_selector,
            },
        }
    }

    pub(super) fn start(
        self,
        initial_msgs: Vec<RemoteStateMessage>,
        tasks: &mut JoinSet<(EndpointId, Vec<RemoteStateMessage>)>,
        shutdown_token: CancellationToken,
        parent_span: Span,
    ) -> mpsc::Sender<RemoteStateMessage> {
        let (tx, rx) = mpsc::channel(16);
        let endpoint_id = self.state.endpoint_id;

        // Ideally we'd use the endpoint span as parent.  We'd have to plug that span into
        // here somehow.  Instead we have no parent and explicitly set the me attribute.  If
        // we don't explicitly set a span we get the spans from whatever call happens to
        // first create the actor, which is often very confusing as it then keeps those
        // spans for all logging of the actor.
        tasks.spawn(
            self.run(initial_msgs, rx, shutdown_token)
                .instrument(info_span!(
                    parent: parent_span,
                    "RemoteStateActor",
                    remote = %endpoint_id.fmt_short(),
                )),
        );
        tx
    }

    /// Runs the main loop of the actor.
    ///
    /// Note that the actor uses async handlers for tasks from the main loop.  The actor is
    /// not processing items from the inbox while waiting on any async calls.  So some
    /// discipline is needed to not turn pending for a long time.
    async fn run(
        mut self,
        initial_msgs: Vec<RemoteStateMessage>,
        mut inbox: mpsc::Receiver<RemoteStateMessage>,
        shutdown_token: CancellationToken,
    ) -> (EndpointId, Vec<RemoteStateMessage>) {
        trace!("actor started");
        for msg in initial_msgs {
            self.handle_message(msg).await;
        }
        let idle_timeout = time::sleep(ACTOR_MAX_IDLE_TIMEOUT);
        n0_future::pin!(idle_timeout);

        let check_connections = time::interval(UPGRADE_INTERVAL);
        n0_future::pin!(check_connections);

        // HYPER PATCH (additive, flag-gated — see [`crate::radio`]): the aligned keepalive.
        //
        // These are deadlines, not intervals, and every one of them is snapped onto the
        // process-wide grid. That is the entire point: this actor, the relay ping and the
        // re-STUN timer all wait for the *same* instant, so they cost one radio wake
        // between them rather than three. A free-running `time::interval` here would
        // re-introduce an independent phase and give the win straight back.
        let mut aligned_tick: Option<Instant> = None;
        let mut aligned_burst2: Option<Instant> = None;

        loop {
            // Re-read the flag every iteration so it can be flipped on a running process.
            if crate::radio::aligned_enabled() {
                if aligned_tick.is_none() {
                    aligned_tick = Some(crate::radio::next_tick());
                }
            } else if aligned_tick.is_some() || aligned_burst2.is_some() {
                aligned_tick = None;
                aligned_burst2 = None;
            }
            let aligned_tick_fut = match aligned_tick {
                Some(when) => MaybeFuture::Some(time::sleep_until(when)),
                None => MaybeFuture::None,
            };
            n0_future::pin!(aligned_tick_fut);
            let aligned_burst2_fut = match aligned_burst2 {
                Some(when) => MaybeFuture::Some(time::sleep_until(when)),
                None => MaybeFuture::None,
            };
            n0_future::pin!(aligned_burst2_fut);

            let scheduled_path_open = match self.state.scheduled_open_path {
                Some(when) => MaybeFuture::Some(time::sleep_until(when)),
                None => MaybeFuture::None,
            };
            n0_future::pin!(scheduled_path_open);
            let scheduled_hp = match self.state.scheduled_holepunch {
                Some(when) => MaybeFuture::Some(time::sleep_until(when)),
                None => MaybeFuture::None,
            };
            n0_future::pin!(scheduled_hp);
            if !self.is_idle(&inbox) {
                idle_timeout
                    .as_mut()
                    .reset(Instant::now() + ACTOR_MAX_IDLE_TIMEOUT);
            }

            tokio::select! {
                biased;

                _ = shutdown_token.cancelled() => {
                    trace!("actor cancelled");
                    break;
                }
                msg = inbox.recv() => {
                    match msg {
                        Some(msg) => self.handle_message(msg).await,
                        None => break,
                    }
                }
                Some((id, evt)) = self.state.path_events.next() => {
                    self.handle_path_event(id, evt);
                }
                Some((id, evt)) = self.state.addr_events.next() => {
                    trace!(?id, ?evt, "remote addrs updated, triggering holepunching");
                    self.trigger_holepunching();
                }
                Some((conn_id, closed)) = self.state.connections_close.next(), if !self.state.connections_close.is_empty() => {
                    self.handle_connection_close(conn_id, closed);
                }
                res = self.state.local_direct_addrs.updated() => {
                    if let Err(n0_watcher::Disconnected) = res {
                        trace!("direct address watcher disconnected, shutting down");
                        break;
                    }
                    self.update_local_direct_address();
                    trace!("local addrs updated, triggering holepunching");
                    self.trigger_holepunching();
                }
                _ = &mut scheduled_path_open => {
                    trace!("triggering scheduled path_open");
                    self.state.scheduled_open_path = None;
                    let mut addrs = std::mem::take(&mut self.state.pending_open_paths);
                    while let Some(addr) = addrs.pop_front() {
                        self.open_path_on_all_conns(&addr);
                    }
                }
                _ = &mut scheduled_hp => {
                    trace!("triggering scheduled holepunching");
                    self.state.scheduled_holepunch = None;
                    self.trigger_holepunching();
                }
                Some(item) = maybe_next(self.state.address_lookup_stream.as_mut()), if self.state.address_lookup_stream.is_some() => {
                    self.state.handle_address_lookup_item(item);
                }
                _ = check_connections.tick() => {
                    self.check_connections();
                    // HYPER PATCH (additive, flag-gated — see [`crate::radio`]): the
                    // adaptive path lifecycle rides this existing tick rather than adding
                    // one. Sweeping AFTER `check_connections` is deliberate: if that call
                    // learned a new candidate it will have stood the sweep down, and the
                    // safe direction to be wrong in is "retire nothing this round".
                    self.retire_idle_paths();
                }
                // HYPER PATCH (additive, flag-gated — see [`crate::radio`]).
                _ = &mut aligned_tick_fut => {
                    let pinged = self.aligned_keepalive_burst();
                    crate::radio::record_burst(pinged);
                    // Second packet of the burst. One dropped keepalive must not cost the
                    // whole interval — that is the difference between a slow tick and a
                    // dead connection — and 300ms later is still inside the same radio
                    // wake, so it is free.
                    aligned_burst2 =
                        (pinged > 0).then(|| Instant::now() + crate::radio::BURST_GAP);
                    aligned_tick = Some(crate::radio::next_tick());
                }
                _ = &mut aligned_burst2_fut => {
                    aligned_burst2 = None;
                    self.aligned_keepalive_burst();
                }
                _ = &mut idle_timeout => {
                    if self.is_idle(&inbox) {
                        trace!("idle timeout expired and still idle: terminate actor");
                        break;
                    } else {
                        // Seems like we weren't really idle, so we reset
                        idle_timeout.as_mut().reset(Instant::now() + ACTOR_MAX_IDLE_TIMEOUT);
                    }
                }
            }
        }

        inbox.close();
        // There might be a race between checking `inbox.is_empty()` and `inbox.close()`,
        // so we pull out all messages that are left over.
        let mut leftover_msgs = Vec::with_capacity(inbox.len());
        inbox.recv_many(&mut leftover_msgs, inbox.len()).await;

        trace!("actor terminating");
        (self.state.endpoint_id, leftover_msgs)
    }

    /// HYPER PATCH (battery leak fix): upstream checks `self.connections.is_empty()`, but a
    /// `ConnectionState` whose strong `noq::Connection` the application already dropped stays in
    /// the map — its `WeakConnectionHandle` can never `upgrade()` again — so the actor is counted
    /// "connected" and NEVER trips the 60s `ACTOR_MAX_IDLE_TIMEOUT` arm. On a churning gossip mesh
    /// those orphaned actors accumulate without bound (the unbounded RemoteMap growth = climbing
    /// background CPU + RSS). We instead treat the actor as having a live connection only if at
    /// least one handle still upgrades. This is safe by construction: any peer whose connection the
    /// app / iroh-gossip still holds strongly upgrades and is never classified idle, so active peers
    /// are never reaped; only fully-orphaned actors self-terminate (and a later inbound datagram
    /// rebuilds one on demand). See `MAX_INACTIVE_NODES` (remote_map.rs, still disabled upstream).
    fn is_idle(&self, inbox: &mpsc::Receiver<RemoteStateMessage>) -> bool {
        let has_live_connection = self
            .connections
            .values()
            .any(|conn| conn.handle.upgrade().is_some());
        !has_live_connection
            && inbox.is_empty()
            && self.state.paths.resolve_requests_is_empty()
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): one keepalive packet on
    /// every open path of every live connection to this remote.
    ///
    /// This replaces noq's autonomous per-path keepalive timers. Doing it here rather than
    /// leaving it to noq is what makes alignment possible: noq would give each path its own
    /// phase, whereas one sweep from one grid-snapped deadline puts every path's PING into
    /// a single radio wake.
    ///
    /// Returns how many paths were actually pinged, which is also the signal for whether a
    /// second burst packet is worth scheduling — with nothing open there is nothing to
    /// retry, and waking the modem again would be pure cost.
    fn aligned_keepalive_burst(&self) -> usize {
        let mut pinged = 0;
        for conn_state in self.connections.values() {
            // A weak handle that no longer upgrades is an orphaned connection the app has
            // already dropped (see `is_idle`); pinging it would resurrect nothing.
            let Some(conn) = conn_state.handle.upgrade() else {
                continue;
            };
            for path_id in conn_state.paths.keys() {
                let Some(path) = conn.path(*path_id) else {
                    continue;
                };
                match path.ping() {
                    Ok(()) => pinged += 1,
                    // A path can close between the map read and the ping; that is routine,
                    // and the path-event stream is what removes it from `paths`.
                    Err(err) => trace!(?err, ?path_id, "aligned keepalive: path closed"),
                }
            }
        }
        trace!(pinged, "aligned keepalive burst");
        pinged
    }

    /// Handles an actor message.
    ///
    /// Error returns are fatal and kill the actor.
    #[instrument(skip(self))]
    async fn handle_message(&mut self, msg: RemoteStateMessage) {
        // trace!("handling message");
        match msg {
            RemoteStateMessage::SendDatagram(sender, transmit) => {
                self.state.handle_msg_send_datagram(sender, transmit).await;
            }
            RemoteStateMessage::AddConnection(handle, tx) => {
                self.handle_msg_add_connection(handle, tx);
            }
            RemoteStateMessage::ResolveRemote(addrs, tx) => {
                self.state.handle_msg_resolve_remote(addrs, tx);
            }
            RemoteStateMessage::RemoteInfo(tx) => {
                let addrs = self.state.paths.to_remote_addrs();
                let info = RemoteInfo {
                    endpoint_id: self.state.endpoint_id,
                    addrs,
                };
                tx.send(info).ok();
            }
            RemoteStateMessage::NetworkChange { is_major } => {
                self.handle_msg_network_change(is_major);
            }
        }
    }

    /// Handles [`RemoteStateMessage::AddConnection`].
    ///
    /// Error returns are fatal and kill the actor.
    fn handle_msg_add_connection(
        &mut self,
        conn: noq::Connection,
        tx: oneshot::Sender<PathStateReceiver>,
    ) {
        let (path_state_sender, path_state_receiver) = PathStateSender::new();
        self.state.metrics.num_conns_opened.inc();
        // Remove any conflicting stable_ids from the local state.
        let conn_id = ConnId(conn.stable_id());
        self.connections.remove(&conn_id);

        // Hook up paths, NAT addresses and connection closed event streams.
        self.state
            .path_events
            .push(Box::pin(conn.path_events().map(move |evt| (conn_id, evt))));
        self.state.addr_events.push(Box::pin(
            conn.nat_traversal_updates().map(move |evt| (conn_id, evt)),
        ));
        self.state.connections_close.push(OnClosed::new(&conn));

        // Add local addrs to the connection
        let local_addrs = self.state.local_candidates();
        update_qnt_candidates(&conn, &local_addrs);

        // Store the connection
        let conn_state = self
            .connections
            .entry(conn_id)
            .insert_entry(ConnectionState {
                handle: conn.weak_handle(),
                path_state: path_state_sender,
                paths: Default::default(),
                has_been_direct: false,
            })
            .into_mut();

        // Store PathId(0), set path_status and select best path, check if holepunching
        // is needed.
        if let Some(path) = conn.path(PathId::ZERO) {
            let path_remote = self
                .state
                .register_and_configure_path(conn_id, conn_state, &path);

            if let Some(path_remote) = path_remote
                && !path_remote.is_relay()
                && conn.side().is_client()
            {
                // We may have raced this with a relay address.  Try and add any
                // relay addresses we have back.
                let relays = self
                    .state
                    .paths
                    .addrs()
                    .filter(|addr| addr.is_relay())
                    .map(|addr| transports::FourTuple::from_remote(addr.clone()))
                    .collect::<Vec<_>>();
                for open_addr in relays {
                    self.state
                        .open_path_on_conn(conn_id, conn_state, &conn, &open_addr);
                }
            }
        }
        self.trigger_holepunching();
        self.select_path();
        tx.send(path_state_receiver).ok();
    }

    /// Handles [`RemoteStateMessage::NetworkChange`].
    fn handle_msg_network_change(&mut self, is_major: bool) {
        // Ping all the paths so loss-detection starts ASAP.
        for conn in self.connections.values() {
            if let Some(noq_conn) = conn.handle.upgrade() {
                for (path_id, addr) in &conn.paths {
                    if let Some(path) = noq_conn.path(*path_id) {
                        // Ping the current path
                        if let Err(err) = path.ping() {
                            warn!(%err, %path_id, ?addr, "failed to ping path");
                        }
                    }
                }
            }
        }

        // HYPER PATCH (additive): the link moved under us, so everything we learned about
        // which candidates are reachable is now stale. Collapse the backoff for BOTH kinds
        // of change — even a minor one can restore reachability, and holding a five-minute
        // window after the network changed is exactly the "stuck on relay" failure the
        // backoff must never cause.
        self.reset_holepunch_backoff("network change");
        // Re-widening signal 1 of 4: the link moved, so the path set we converged on may
        // describe a network that no longer exists. Stop narrowing until we have re-probed.
        self.arm_path_widening("network change");
        // HYPER PATCH (additive, flag-gated — see [`crate::radio`]): and give every
        // candidate its speculative probe budget back. A link change is the ONLY event that
        // re-arms the whole table, because it is the only one that invalidates every
        // judgement at once: the addresses we proved silent were proved silent on a network
        // that is no longer the one we are on. Every other re-arm is per-address and
        // evidence-driven, which is what stops a five-minute pkarr republish from quietly
        // undoing the budget. Kept separate from `arm_path_widening` — same signal, but the
        // two features are independently flagged and must stay independently revertible.
        let rearmed = self.state.paths.rearm_fanout();
        if rearmed > 0 {
            debug!(rearmed, "fan-out budget: re-armed by network change");
        }

        if is_major {
            self.trigger_holepunching();
        }
    }

    fn handle_connection_close(&mut self, conn_id: ConnId, closed: Closed) {
        event!(
            target: "iroh::_events::conn::closed",
            Level::DEBUG,
            %conn_id,
            remote_id = %self.state.endpoint_id.fmt_short(),
            reason=?closed.reason,
        );

        if let Some(conn_state) = self.connections.remove(&conn_id) {
            self.state.metrics.num_conns_closed.inc();
            conn_state.path_state.close(closed);
        }
        if self.connections.is_empty() {
            trace!("last connection closed - clearing selected_path");
            self.state.selected_path = None;
        }
    }

    /// Updates the local [`DirectAddr`]s to all connections.
    ///
    /// Each connection needs to have the local direct addresses to use as QNT address
    /// candidates.
    fn update_local_direct_address(&mut self) {
        let local_addrs = self.state.local_candidates();
        for conn in self.connections.values().filter_map(|s| s.handle.upgrade()) {
            update_qnt_candidates(&conn, &local_addrs);
        }
        // todo: trace
    }

    /// Triggers holepunching to the remote endpoint.
    ///
    /// This will manage the entire process of holepunching with the remote endpoint.
    ///
    /// - Holepunching happens on the Connection with the lowest [`ConnId`] which is a
    ///   client.
    ///   - Both endpoints may initiate holepunching if both have a client connection.
    ///   - Any opened paths are opened on all other connections without holepunching.
    /// - If there are no changes in local or remote candidate addresses since the
    ///   last attempt **and** there was a recent attempt, a trigger_holepunching call
    ///   will be scheduled instead.
    fn trigger_holepunching(&mut self) {
        if self.connections.is_empty() {
            trace!("not holepunching: no connections");
            return;
        }

        let Some(conn) = self
            .connections
            .iter()
            .filter_map(|(id, state)| state.handle.upgrade().map(|conn| (*id, conn)))
            .filter(|(_, conn)| conn.side().is_client())
            .min_by_key(|(id, _)| *id)
            .map(|(_, conn)| conn)
        else {
            trace!("not holepunching: no client connection");
            return;
        };
        let remote_candidates = match conn.get_remote_nat_traversal_addresses() {
            Ok(addrs) => BTreeSet::from_iter(addrs),
            Err(err) => {
                warn!("failed to get nat candidate addresses: {err:#}");
                return;
            }
        };
        let local_candidates = self.state.local_candidates();
        let new_candidates = self
            .state
            .last_holepunch
            .as_ref()
            .map(|last_hp| {
                // Addrs are allowed to disappear, but if there are new ones we need to
                // holepunch again.
                trace!(
                    ?last_hp,
                    ?local_candidates,
                    ?remote_candidates,
                    "candidates to holepunch?"
                );
                !remote_candidates.is_subset(&last_hp.remote_candidates)
                    || !local_candidates.is_subset(&last_hp.local_candidates)
            })
            .unwrap_or(true);
        if new_candidates {
            // We learned something. Any backoff we had built up is stale — a new candidate
            // is exactly the evidence that a retry might now succeed.
            self.state.holepunch_idle_rounds = 0;
            // Re-widening signal 2 of 4: a candidate we have never tried is about to be
            // dialled. Retiring paths while that is in flight would fight the probe.
            self.arm_path_widening("new candidates");
        } else if let Some(ref last_hp) = self.state.last_holepunch {
            // HYPER PATCH (additive, flag-gated — see [`crate::radio`]): back off blind
            // retries instead of repeating them at a fixed floor forever.
            //
            // Upstream waits only `HOLEPUNCH_ATTEMPTS_INTERVAL` (5s) here. That is a floor,
            // not a backoff: `check_connections` calls us every `UPGRADE_INTERVAL` (60s),
            // by which time the floor has always expired, so we fall through and re-punch
            // with a candidate set we already know does not work. See
            // `State::holepunch_idle_rounds`.
            let wait = if crate::radio::holepunch_backoff_enabled() {
                crate::radio::holepunch_backoff(self.state.holepunch_idle_rounds)
            } else {
                HOLEPUNCH_ATTEMPTS_INTERVAL
            };
            let next_hp = last_hp.when + wait;
            let now = Instant::now();
            if next_hp > now {
                trace!(scheduled_in = ?(next_hp - now), "not holepunching: no new addresses");
                self.state.scheduled_holepunch = Some(next_hp);
                return;
            }
            // We are about to re-punch with nothing new to go on. Widen the window before
            // the next one, so a peer we can never reach directly costs us a round every
            // five minutes rather than every sixty seconds.
            self.state.holepunch_idle_rounds = self.state.holepunch_idle_rounds.saturating_add(1);
            debug!(
                rounds = self.state.holepunch_idle_rounds,
                ?wait,
                "holepunching with no new candidates"
            );
        }

        self.state.do_holepunching(conn);
    }

    /// HYPER PATCH (additive): does ANY connection to this remote still have an IP path?
    ///
    /// Used to decide whether losing a path just cost us our last direct route, in which
    /// case re-punching must happen immediately rather than after the backoff.
    fn has_any_ip_path(&self) -> bool {
        self.connections
            .values()
            .flat_map(|conn| conn.paths.values())
            .any(|network_path| network_path.is_ip())
    }

    /// HYPER PATCH (additive): collapse the holepunch backoff and re-punch now.
    ///
    /// Called on the events that make a retry genuinely worth attempting again. Clearing
    /// `scheduled_holepunch` matters as much as clearing the counter: a pending far-future
    /// wake-up would otherwise still gate the attempt.
    fn reset_holepunch_backoff(&mut self, why: &'static str) {
        if self.state.holepunch_idle_rounds == 0 {
            return;
        }
        debug!(
            rounds = self.state.holepunch_idle_rounds,
            why, "resetting holepunch backoff"
        );
        self.state.holepunch_idle_rounds = 0;
        self.state.scheduled_holepunch = None;
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): stand the path-lifecycle
    /// sweep down, because something just happened that invalidates what we know about
    /// reachability.
    ///
    /// This is the ONLY way the sweep is ever suspended, and it is called from exactly four
    /// places, all of them real network signals rather than a schedule: a link change, a
    /// newly-learned candidate address, a direct path being established, and the loss of
    /// our last direct path. That is what stops the lifecycle from narrowing us permanently
    /// onto a path that later dies — the events that would strand us are precisely the
    /// events that re-open the window.
    ///
    /// Kept separate from [`Self::reset_holepunch_backoff`] even though they share every
    /// call site: the two features are independently flagged, and coupling them would make
    /// it impossible to measure or revert one without the other.
    fn arm_path_widening(&mut self, why: &'static str) {
        if !crate::radio::path_lifecycle_enabled() {
            return;
        }
        let until = Instant::now() + crate::radio::PATH_WIDEN_HOLD;
        // Only ever extend the hold. Two signals in quick succession mean *more*
        // uncertainty, not less, so the later deadline is the correct one.
        if self.state.lifecycle_hold_until.is_none_or(|prev| until > prev) {
            trace!(why, "path lifecycle: widening, sweep stood down");
            self.state.lifecycle_hold_until = Some(until);
        }
    }

    /// HYPER PATCH (additive, flag-gated — see [`crate::radio`]): retire IP paths that have
    /// stopped being useful.
    ///
    /// Runs off the existing `UPGRADE_INTERVAL` tick rather than a timer of its own — the
    /// whole point of this work is to stop waking the radio, so adding a wake to save
    /// packets would be self-defeating.
    fn retire_idle_paths(&mut self) {
        if !crate::radio::path_lifecycle_enabled() {
            return;
        }
        if let Some(until) = self.state.lifecycle_hold_until {
            if Instant::now() < until {
                trace!("path lifecycle: still widening, not sweeping");
                return;
            }
            self.state.lifecycle_hold_until = None;
        }

        // Anything live on any connection is off limits, whatever `PathStatus` says about
        // it. The recorded status can lag reality — closing a whole connection does not
        // mark its paths abandoned — and retiring a path we are actively sending on would
        // be a delivery bug, not an efficiency win.
        let in_use: std::collections::HashSet<transports::Addr> = self
            .connections
            .values()
            .flat_map(|conn| conn.paths.values())
            .map(|network_path| network_path.remote())
            .collect();

        let retired = self.state.paths.retire_idle_paths(&in_use);
        if retired > 0 {
            crate::radio::record_paths_retired(retired);
            debug!(retired, "path lifecycle: retired idle paths");
        }
    }

    #[instrument(skip(self))]
    fn handle_path_event(&mut self, conn_id: ConnId, event: Result<NoqPathEvent, noq::Lagged>) {
        let Ok(event) = event else {
            warn!("missed a PathEvent, RemoteStateActor lagging");
            // TODO: Is it possible to recover using the sync APIs to figure out what the
            //    state of the connection and it's paths are?
            return;
        };
        let Some(conn_state) = self.connections.get_mut(&conn_id) else {
            trace!("event for removed connection");
            return;
        };
        let Some(conn) = conn_state.handle.upgrade() else {
            trace!("event for closed connection");
            return;
        };
        trace!("path event");
        match event {
            NoqPathEvent::Established { id: path_id, .. } => {
                let Some(path) = conn.path(path_id) else {
                    trace!("path open event for unknown path");
                    return;
                };

                let opened = self
                    .state
                    .register_and_configure_path(conn_id, conn_state, &path);
                self.select_path();
                // HYPER PATCH (additive): a direct path came up, so holepunching is
                // evidently working against this peer right now. Forget any backoff we had
                // accumulated while it was not.
                if opened.is_some_and(|network_path| network_path.is_ip()) {
                    self.reset_holepunch_backoff("ip path established");
                    // Re-widening signal 3 of 4: holepunching just worked. The path set is
                    // actively changing shape, so let it settle before narrowing again.
                    self.arm_path_widening("ip path established");
                }
            }
            NoqPathEvent::Abandoned { id, reason, .. } => {
                // Remove abandoned path from the conn state.
                let Some(network_path) = conn_state.remove_path(&id, &conn) else {
                    debug!(%id, "path not in path_id_map");
                    return;
                };
                // HYPER PATCH (additive): note this before the borrow ends; the "is this our
                // last direct path?" check below needs all connections, not just this one.
                let lost_ip_path = network_path.is_ip();

                // We track all known remote addresses for the peer in `State::paths`. The paths are tracked
                // by remote address only (we ignore the local IP). Therefore, we mark a remote addr as abandoned
                // in the remote-global state only once no connections have any path to that remote addr.
                if !conn_state
                    .paths
                    .values()
                    .any(|tuple| tuple.remote() == network_path.remote())
                {
                    self.state.paths.abandoned_path(&network_path.remote());
                }

                event!(
                    target: "iroh::_events::path::abandoned",
                    Level::DEBUG,
                    remote = %self.state.endpoint_id.fmt_short(),
                    %conn_id,
                    path_id = %id,
                    %network_path,
                    ?reason
                );

                // If the remote closed our selected path, select a new one.
                self.select_path();

                // HYPER PATCH (additive): losing our LAST direct path is the one case where
                // waiting out the backoff would be felt by the user — they would sit on
                // relay-only latency for up to the cap. Re-punch immediately instead.
                //
                // Deliberately conditional on no IP path remaining anywhere: while another
                // direct path is still up we have not lost the capability, so a teardown of
                // one redundant path must not re-arm the churn we are trying to stop.
                if lost_ip_path && !self.has_any_ip_path() {
                    self.reset_holepunch_backoff("lost last ip path");
                    // Re-widening signal 4 of 4: THE one that makes narrowing safe. If the
                    // path we converged onto dies, we must not still be holding a narrow
                    // table — this re-opens the window for the replacement before any
                    // further retirement can happen.
                    self.arm_path_widening("lost last ip path");
                    self.trigger_holepunching();
                }
            }
            NoqPathEvent::Discarded { id, path_stats, .. } => {
                trace!(%id, ?path_stats, "path discarded");
            }
            NoqPathEvent::RemoteStatus { .. } | NoqPathEvent::ObservedAddr { .. } => {
                // Nothing to do for these events.
            }
            _ => {
                // We expect to keep noq and iroh in sync in all test setups, but in production it's totally possible
                // that iroh itself is linked against a newer version of noq with additional events we don't yet
                // know how to handle.
                #[cfg(test)]
                panic!("Unhandled path event: {event:?}");
            }
        }
    }

    /// Selects the preferred path by invoking the configured [`PathSelector`].
    ///
    /// The selected path is added to any connections which do not yet have it.  Any unused
    /// direct paths are closed for all connections.
    #[instrument(skip_all)]
    fn select_path(&mut self) {
        let current_path = self.state.selected_path.as_ref();
        let selected_addr = {
            let ctx = PathSelectionContext::new(current_path, &self.connections);
            self.state.path_selector.select(&ctx).selected().cloned()
        };

        if let Some(addr) = selected_addr
            && self.state.selected_path.as_ref() != Some(&addr)
        {
            let prev_remote = self.state.selected_path.replace(addr.clone());
            event!(
                target: "iroh::_events::path::selected",
                Level::DEBUG,
                remote = %self.state.endpoint_id.fmt_short(),
                network_path = %addr,
                prev_network_path = %prev_remote.map(|p| format!("{p}")).unwrap_or("None".to_string()),
            );
        } else {
            trace!(?current_path, "keeping current path");
        }

        self.apply_selected_path();
    }

    /// Propagates a change of [`State::selected_path`] to noq.
    ///
    /// Iterates over all connections and applies the selected path as follows:
    /// - Closes non-selected IP paths (but keeps one IP path open still)
    /// - Sets all non-selected paths to [`PathStatus::Backup`]
    /// - Opens the selected path if it does not exist on the connection
    /// - Sets the selected path to [`PathStatus::Available`]
    fn apply_selected_path(&mut self) {
        let Some(selected) = self.state.selected_path.clone() else {
            // We can't open the selected path on all paths if we don't have one yet.
            // And we can't close all "unselected" paths either, because we don't know which one is selected.
            return;
        };

        for (conn_id, conn_state) in self.connections.iter() {
            let Some(conn) = conn_state.handle.upgrade() else {
                continue;
            };

            // Open path if it doesn't exist yet.
            self.state
                .open_path_on_conn(*conn_id, conn_state, &conn, &selected);

            for (path_id, path_remote) in conn_state.paths.iter() {
                let Some(path) = conn.path(*path_id) else {
                    continue;
                };

                // Closes redundant IP paths so that at most one remains per connection.
                //
                // Relay and custom paths are kept open. Only the client closes paths,
                // to avoid the client and server independently closing different paths
                // and racing to abandon the last one.
                if conn.side().is_client()
                    && path_remote.is_ip()
                    && path_remote != &selected
                    && conn_state.paths.values().filter(|a| a.is_ip()).count() > 1
                {
                    trace!(?path_remote, %conn_id, %path_id, "closing direct path");
                    match path.close() {
                        Err(noq_proto::ClosePathError::MultipathNotNegotiated) => {
                            error!("multipath not negotiated");
                        }
                        Err(noq_proto::ClosePathError::LastOpenPath) => {
                            error!("could not close last open path");
                        }
                        Err(noq_proto::ClosePathError::ClosedPath) => {
                            // We already closed this.
                        }
                        Ok(()) => {}
                    }
                    continue;
                }

                // Set path status: The selected path becomes Available, all other paths become Backup.
                self.state.set_path_status(*conn_id, &path, path_remote);
            }

            // Record the new selected path in the path watcher.
            conn_state.path_state.record_selected(&selected);
        }
    }

    fn open_path_on_all_conns(&mut self, open_addr: &transports::FourTuple) {
        for (conn_id, conn_state) in self.connections.iter() {
            let Some(conn) = conn_state.handle.upgrade() else {
                continue;
            };
            self.state
                .open_path_on_conn(*conn_id, conn_state, &conn, open_addr);
        }
    }

    /// Handles regularly checking if any paths need hole punching currently
    ///
    /// Currently we need to have 1 IP path, with a good enough latency.
    fn check_connections(&mut self) {
        let mut is_goodenough = true;
        for conn_state in self.connections.values() {
            let mut is_conn_goodenough = false;
            if let Some(conn) = conn_state.handle.upgrade() {
                let min_ip_rtt = conn_state
                    .paths
                    .iter()
                    .filter_map(|(path_id, addr)| {
                        if addr.is_ip() {
                            conn.path_stats(*path_id).map(|stats| stats.rtt)
                        } else {
                            None
                        }
                    })
                    .min();

                if let Some(min_ip_rtt) = min_ip_rtt {
                    let is_latency_goodenough = min_ip_rtt <= GOOD_ENOUGH_LATENCY;
                    is_conn_goodenough = is_latency_goodenough;
                } else {
                    // No IP transport found
                    is_conn_goodenough = false;
                }
            }
            is_goodenough &= is_conn_goodenough;
        }

        if !is_goodenough {
            debug!("connections are not good enough, triggering holepunching");
            self.trigger_holepunching();
        }
    }
}

impl State {
    /// Handles [`RemoteStateMessage::SendDatagram`].
    async fn handle_msg_send_datagram(
        &mut self,
        mut sender: Box<TransportsSender>,
        transmit: OwnedTransmit,
    ) {
        // Sending datagrams might fail, e.g. because we don't have the right transports set
        // up to handle sending this owned transmit to.
        // After all, we try every single path that we know (relay URL, IP address), even
        // though we might not have a relay transport or ip-capable transport set up.
        // So these errors must not be fatal for this actor (or even this operation).

        if let Some(addr) = self.selected_path.as_ref() {
            trace!(?addr, "sending datagram to selected path");

            // TODO(Frando): We might want to include a local IP here in the future, if we confidently
            // know that it is the correct one.
            // See https://github.com/n0-computer/iroh/issues/4280.
            let four_tuple = transports::FourTuple::from_remote(addr.remote());
            if let Err(err) = send_datagram(&mut sender, four_tuple, transmit).await {
                debug!(?addr, "failed to send datagram on selected_path: {err:#}");
            }
        } else {
            trace!(
                paths = ?self.paths.addrs().collect::<Vec<_>>(),
                "sending datagram to all known paths",
            );
            if self.paths.is_empty() {
                warn!("Cannot send datagrams: No paths to remote endpoint known");
            }

            // HYPER PATCH (additive, flag-gated — see [`crate::radio`]): upstream sends one
            // copy to EVERY address in the table here. On a CGNAT handset that table
            // saturates at 30 dead private candidates, so a single connect costs
            // `30 destinations x 8 packets` (the Initial plus its PTO ladder) for a connect
            // that only ever completes over the relay — measured at 3.649 pkt/s and 66% of
            // all transmitted bytes at idle.
            //
            // `fanout_addrs` applies a per-candidate probe budget instead: relay and custom
            // addresses always, unproven candidates always, and proven-silent ones on a
            // rotating re-probe window. With the flag off it returns the whole table in the
            // same order, so this loop is unchanged. See `select_fanout_addrs`.
            for addr in self.paths.fanout_addrs() {
                // We never want to send to our local addresses.
                // The local address set is updated in the main loop so we can use `peek` here.
                if let transports::Addr::Ip(sockaddr) = &addr
                    && self
                        .local_direct_addrs
                        .peek()
                        .iter()
                        .any(|a| a.addr == *sockaddr)
                {
                    trace!(%sockaddr, "not sending datagram to our own address");

                // TODO(Frando): We might want to include a local IP here in the future, if we confidently
                // know that it is the correct one.
                // See https://github.com/n0-computer/iroh/issues/4280.
                } else if let Err(err) = send_datagram(
                    &mut sender,
                    transports::FourTuple::from_remote(addr.clone()),
                    transmit.clone(),
                )
                .await
                {
                    debug!(?addr, "failed to send datagram: {err:#}");
                }
            }
            // This message is received *before* a connection is added.  So we do
            // not yet have a connection to holepunch.  Instead we trigger
            // holepunching when AddConnection is received.
        }
    }

    /// Handles [`RemoteStateMessage::ResolveRemote`].
    fn handle_msg_resolve_remote(
        &mut self,
        addrs: BTreeSet<TransportAddr>,
        tx: oneshot::Sender<Result<(), AddressLookupFailed>>,
    ) {
        let addrs = to_transports_addr(self.endpoint_id, addrs);
        self.paths.insert_multiple(addrs, Source::App);
        self.paths.resolve_remote(tx);
        // Start Address Lookup if we have no selected path.
        self.trigger_address_lookup();
    }

    /// Triggers Address Lookup for the remote endpoint, if needed.
    ///
    /// Does not start Address Lookup if we have a selected path or if Address Lookup is
    /// currently running.
    fn trigger_address_lookup(&mut self) {
        if self.selected_path.is_some() || self.address_lookup_stream.is_some() {
            return;
        }
        let stream = self.address_lookup.resolve(self.endpoint_id);
        let stream = stream.filter_map(|item| match item {
            // We don't care about errors from individual services, we just continue.
            // Individual errors are buffered into the final error by `AddressLookupServices::resolve`,
            // and if the lookup fails we return them upstream with the final `AddressLookupFailed` error.
            Ok(Err(_err)) => None,
            Ok(Ok(item)) => Some(Ok(item)),
            Err(err) => Some(Err(err)),
        });
        self.address_lookup_stream = Some(Box::pin(stream));
    }

    /// Handles an address lookup result.
    ///
    /// All address lookup results end up being sent here. It takes care of updating the
    /// [`RemotePathState`] with the results.
    fn handle_address_lookup_item(
        &mut self,
        item: Option<Result<AddressLookupItem, AddressLookupFailed>>,
    ) {
        match item {
            None => {
                self.paths.address_lookup_finished(Ok(()));
                self.address_lookup_stream = None;
            }
            Some(Err(err)) => {
                if let AddressLookupFailed::NoServiceConfigured { .. } = err {
                    trace!("Address Lookup not configured");
                } else {
                    debug!("Address Lookup failed: {err:#}");
                }
                self.paths.address_lookup_finished(Err(err));
                self.address_lookup_stream = None;
            }
            Some(Ok(item)) => {
                if item.endpoint_id() != self.endpoint_id {
                    warn!(
                        ?item,
                        "Address Lookup emitted item for wrong remote endpoint"
                    );
                } else {
                    let source = Source::AddressLookup {
                        name: item.provenance().to_string(),
                    };
                    let addrs =
                        to_transports_addr(self.endpoint_id, item.into_endpoint_addr().addrs);
                    self.paths.insert_multiple(addrs, source);
                }
            }
        }
    }

    /// Unconditionally perform holepunching.
    #[instrument(skip_all)]
    fn do_holepunching(&mut self, conn: noq::Connection) {
        self.metrics.holepunch_attempts.inc();
        let local_candidates = self.local_candidates();
        match conn.initiate_nat_traversal_round() {
            Ok(remote_candidates) => {
                let remote_candidates = remote_candidates
                    .iter()
                    .map(|addr| SocketAddr::new(addr.ip().to_canonical(), addr.port()))
                    .collect();
                event!(
                    target: "iroh::_events::qnt::init",
                    Level::DEBUG,
                    remote = %self.endpoint_id.fmt_short(),
                    ?local_candidates,
                    ?remote_candidates,
                );
                self.last_holepunch = Some(HolepunchAttempt {
                    when: Instant::now(),
                    local_candidates,
                    remote_candidates,
                });
            }
            Err(err) => {
                debug!("failed to initiate NAT traversal: {err:#}");
                use noq_proto::n0_nat_traversal::Error;
                match err {
                    Error::Closed
                    | Error::TooManyAddresses
                    | Error::WrongConnectionSide
                    | Error::ExtensionNotNegotiated => {
                        // Fatal, no need to retry for now
                    }
                    Error::Multipath(_) | Error::NotEnoughAddresses => {
                        // Retry in a bit
                        let now = Instant::now();
                        let next_hp = now + Duration::from_millis(100);
                        trace!(scheduled_in = ?(next_hp - now), "holepunching retry");
                        self.scheduled_holepunch = Some(next_hp);
                    }
                }
            }
        }
    }

    /// Register a path with our state and configure path-specific settings.
    ///
    /// This inserts the path in the [`ConnectionState`] and [`Self::paths`].
    ///
    /// It configures the path with the correct path status (see [`Self::set_path_status`]),
    /// and applies path-type-specific settings:
    /// Relay paths get a longer idle timeout to accommodate transparent reconnection
    /// by the relay actor (see [`RELAY_PATH_MAX_IDLE_TIMEOUT`]).
    fn register_and_configure_path(
        &mut self,
        conn_id: ConnId,
        conn_state: &mut ConnectionState,
        path: &noq::Path,
    ) -> Option<transports::FourTuple> {
        let network_path = self.transport_tuple_for_path(path)?;
        event!(
            target: "iroh::_events::path::open",
            Level::DEBUG,
            remote = %self.endpoint_id.fmt_short(),
            %conn_id,
            path_id=%path.id(),
            %network_path,
        );
        conn_state.add_open_path(network_path.clone(), path.id(), &self.metrics);
        if crate::radio::aligned_enabled() {
            // HYPER PATCH (additive, flag-gated — see `crate::radio`): this path's
            // keepalive is driven by the aligned tick in `RemoteStateActor::run`, not by
            // noq's own per-path timer. A path opened on a connection that was built before
            // the flag was flipped still carries the 5s interval in its config, so clear it
            // here as well as in the transport config — otherwise one straggler path is
            // enough to pin the modem for everyone.
            if let Err(e) = path.set_keep_alive_interval(None) {
                debug!(?e, "failed to clear path keepalive for the aligned tick");
            }
            // Both path kinds get the same generous idle timeout: the relay path needed a
            // longer one anyway (the relay actor reconnects transparently underneath it),
            // and a direct path now has to survive two whole missed ticks.
            if let Err(e) = path.set_max_idle_timeout(Some(crate::radio::ALIGNED_PATH_MAX_IDLE_TIMEOUT))
            {
                debug!(?e, "failed to set aligned path idle timeout");
            }
        } else if network_path.is_relay()
            && let Err(e) = path.set_max_idle_timeout(Some(RELAY_PATH_MAX_IDLE_TIMEOUT))
        {
            debug!(?e, "failed to set relay path idle timeout");
        }

        self.set_path_status(conn_id, path, &network_path);
        self.paths
            .insert_open_path(network_path.remote(), Source::Connection);
        Some(network_path)
    }

    fn set_path_status(
        &mut self,
        conn_id: ConnId,
        path: &noq::Path,
        network_path: &transports::FourTuple,
    ) {
        let status = self.path_status_for_addr(network_path);
        match path.set_status(status) {
            Err(error) => warn!(?error, ?network_path, ?status, "set_status failed"),
            Ok(prev_status) if prev_status != status => {
                event!(
                    target: "iroh::_events::path::set_status",
                    Level::DEBUG,
                    remote = %self.endpoint_id.fmt_short(),
                    %conn_id,
                    path_id=%path.id(),
                    %network_path,
                    ?status,
                    ?prev_status,
                );
            }
            Ok(_) => {}
        }
    }

    fn open_path_on_conn(
        &mut self,
        conn_id: ConnId,
        conn_state: &ConnectionState,
        conn: &noq::Connection,
        open_addr: &transports::FourTuple,
    ) {
        // Only the client opens paths; the server receives them via
        // QUIC frames and reacts to PathOpened events.
        if conn.side().is_server() {
            return;
        }
        // Already open on this connection; nothing to do.
        if conn_state.paths.values().any(|a| a == open_addr) {
            return;
        }

        let quic_addr =
            open_addr.to_noq_four_tuple(&self.relay_mapped_addrs, &self.custom_mapped_addrs);
        let path_status = self.path_status_for_addr(open_addr);

        let fut = conn.open_path_ensure(quic_addr, path_status);
        match fut.path_id() {
            Some(path_id) => {
                trace!(%conn_id, %path_id, ?path_status, "opening new path");
            }
            None => {
                let ret = now_or_never(fut);
                match ret {
                    Some(Err(PathError::RemoteCidsExhausted))
                    | Some(Err(PathError::MaxPathIdReached)) => {
                        self.scheduled_open_path =
                            Some(Instant::now() + Duration::from_millis(333));
                        self.pending_open_paths.push_back(open_addr.clone());
                        trace!(?open_addr, ?ret, "scheduling open_path");
                    }
                    _ => warn!(?ret, "Opening path failed"),
                }
            }
        }
    }

    /// Returns the [`PathStatus`] for `addr`.
    ///
    /// Returns [`PathStatus::Available`] if `addr` is the currently-selected path,
    /// or [`PathStatus::Backup`] otherwise.
    fn path_status_for_addr(&self, addr: &transports::FourTuple) -> PathStatus {
        if Some(addr) == self.selected_path.as_ref() {
            PathStatus::Available
        } else {
            PathStatus::Backup
        }
    }

    /// Returns the [`transports::FourTuple] for a path.
    fn transport_tuple_for_path(&self, path: &noq::Path) -> Option<transports::FourTuple> {
        let noq_network_path = path.network_path().ok()?;
        transports::FourTuple::from_noq(
            noq_network_path,
            &self.relay_mapped_addrs,
            &self.custom_mapped_addrs,
        )
    }

    /// Returns the current set of local direct addresses.
    fn local_candidates(&mut self) -> BTreeSet<SocketAddr> {
        self.local_direct_addrs
            .get()
            .iter()
            .map(|d| d.addr)
            .collect()
    }
}

/// Updates QNT's candidate addresses to be the current set of direct addresses.
///
/// `direct_addrs` must be a set of addresses extracted from the endpoint's current
/// [`DirectAddr`]s.
fn update_qnt_candidates(conn: &noq::Connection, direct_addrs: &BTreeSet<SocketAddr>) {
    let noq_candidates = match conn.get_local_nat_traversal_addresses() {
        Ok(addrs) => BTreeSet::from_iter(addrs),
        Err(err) => {
            warn!("failed to get local nat candidates: {err:#}");
            return;
        }
    };
    for addr in direct_addrs.difference(&noq_candidates) {
        if let Err(err) = conn.add_nat_traversal_address(*addr) {
            warn!("failed adding local addr: {err:#}",);
        }
    }
    for addr in noq_candidates.difference(direct_addrs) {
        if let Err(err) = conn.remove_nat_traversal_address(*addr) {
            warn!("failed removing local addr: {err:#}");
        }
    }
    trace!(?direct_addrs, "updated local QNT addresses");
}

fn send_datagram<'a>(
    sender: &'a mut TransportsSender,
    addr: transports::FourTuple,
    owned_transmit: OwnedTransmit,
) -> impl Future<Output = n0_error::Result<()>> + 'a {
    std::future::poll_fn(move |cx| {
        let transmit = transports::Transmit {
            ecn: owned_transmit.ecn,
            contents: owned_transmit.contents.as_ref(),
            segment_size: owned_transmit.segment_size,
        };

        Pin::new(&mut *sender)
            .poll_send(cx, &addr, &transmit)
            .map(|res| res.with_context(|_| format!("failed to send datagram to {:?}", addr)))
    })
}

/// Messages to send to the [`RemoteStateActor`].
#[derive(derive_more::Debug)]
pub(crate) enum RemoteStateMessage {
    /// Sends a datagram to all known paths.
    ///
    /// Used to send QUIC Initial packets.  If there is no working direct path this will
    /// trigger holepunching.
    ///
    /// This is not acceptable to use on the normal send path, as it is an async send
    /// operation with a bunch more copying.  So it should only be used for sending QUIC
    /// Initial packets.
    #[debug("SendDatagram(..)")]
    SendDatagram(Box<TransportsSender>, OwnedTransmit),
    /// Adds an active connection to this remote endpoint.
    ///
    /// The actor will downgrade the connection to a [`noq::WeakConnectionHandle`] as soon
    /// as it processes the message. It will keep hold of the weak handle until it closes,
    /// but only update to a strong [`noq::Connection`] for brief moments.
    ///
    /// The actor will actively manage paths on the connection and start holepunching as needed.
    #[debug("AddConnection({})", _0.stable_id())]
    AddConnection(noq::Connection, oneshot::Sender<PathStateReceiver>),
    /// Asks if there is any possible path that could be used.
    ///
    /// This adds the provided transport addresses to the list of potential paths for this
    /// remote and starts Address Lookup if needed.
    ///
    /// Sends back `Ok` immediately if the provided address list is non-empy or we have are
    /// other known paths.  Otherwise sends back `Ok` once Address Lookup produces a result,
    /// or the Address Lookup error if Address Lookup fails or produces no results,
    #[debug("ResolveRemote(..)")]
    ResolveRemote(
        BTreeSet<TransportAddr>,
        oneshot::Sender<Result<(), AddressLookupFailed>>,
    ),
    /// Returns information about the remote.
    ///
    /// This currently only includes a list of all known transport addresses for the remote.
    RemoteInfo(oneshot::Sender<RemoteInfo>),
    /// The network status has changed in some way
    NetworkChange { is_major: bool },
}

/// Information about a holepunch attempt.
///
/// Addresses are always stored in canonical form.
#[derive(Debug)]
struct HolepunchAttempt {
    when: Instant,
    /// The set of local addresses which could take part in holepunching.
    ///
    /// This does not mean every address here participated in the holepunching.  E.g. we
    /// could have tried only a sub-set of the addresses because a previous attempt already
    /// covered part of the range.
    ///
    /// We do not store this as a [`DirectAddr`] because this is checked for equality and we
    /// do not want to compare the sources of these addresses.
    local_candidates: BTreeSet<SocketAddr>,
    /// The set of remote addresses which could take part in holepunching.
    ///
    /// Like [`Self::local_candidates`] we may not have used them.
    remote_candidates: BTreeSet<SocketAddr>,
}

/// Newtype to track Connections.
///
/// The wrapped value is the [`noq::Connection::stable_id`] value, and is thus only valid
/// for active connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Display)]
#[display("{_0}")]
struct ConnId(usize);

/// State about one connection.
#[derive(Debug)]
struct ConnectionState {
    /// Weak handle to the connection.
    handle: WeakConnectionHandle,
    /// Writer-side handle for the connection's path observation state.
    ///
    /// The matching [`PathStateReceiver`] is held by the [`Connection`].
    ///
    /// [`Connection`]: crate::endpoint::Connection
    path_state: PathStateSender,
    /// The open paths that exist on this connection.
    paths: FxHashMap<PathId, transports::FourTuple>,
    /// Whether this connection has ever had a direct path.
    ///
    /// Used for recording metrics.
    has_been_direct: bool,
}

impl ConnectionState {
    /// Tracks an open path for the connection.
    fn add_open_path(
        &mut self,
        network_path: transports::FourTuple,
        path_id: PathId,
        metrics: &Arc<SocketMetrics>,
    ) {
        match network_path {
            transports::FourTuple::Ip { .. } => metrics.paths_direct.inc(),
            transports::FourTuple::Relay { .. } => metrics.paths_relay.inc(),
            transports::FourTuple::Custom { .. } => metrics.paths_custom.inc(),
        };
        if !self.has_been_direct && network_path.is_ip() {
            self.has_been_direct = true;
            metrics.num_conns_direct.inc();
        }

        self.paths.insert(path_id, network_path.clone());
        if let Some(conn) = self.handle.upgrade()
            && let Some(path) = conn.path(path_id)
        {
            let handle = path.weak_handle();
            self.path_state.record_opened(handle, network_path);
        }
    }

    /// Removes a path from this connection.
    fn remove_path(
        &mut self,
        path_id: &PathId,
        conn: &noq::Connection,
    ) -> Option<transports::FourTuple> {
        let addr = self.paths.remove(path_id)?;
        self.path_state.record_abandoned(*path_id, conn);
        Some(addr)
    }
}

/// State of the endpoint relevant for path selection.
///
/// Constructed by the endpoint and passed to [`PathSelector::select`].  Borrows from
/// the endpoint's internal data.
#[derive(Debug)]
#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
pub struct PathSelectionContext<'a> {
    current: Option<&'a transports::FourTuple>,
    source: PathsSource<'a>,
}

/// Either a reference to live connection state, or a synthesized list of paths
/// (for unit-testing selectors).
#[derive(Debug)]
enum PathsSource<'a> {
    Live(&'a FxHashMap<ConnId, ConnectionState>),
    #[cfg(test)]
    Test(Vec<PathSelectionData<'a>>),
}

#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
impl<'a> PathSelectionContext<'a> {
    fn new(
        current: Option<&'a transports::FourTuple>,
        connections: &'a FxHashMap<ConnId, ConnectionState>,
    ) -> Self {
        Self {
            current,
            source: PathsSource::Live(connections),
        }
    }

    /// Constructs a context with synthetic path data for testing.
    #[cfg(test)]
    pub(crate) fn for_test(
        current: Option<&'a transports::FourTuple>,
        paths: Vec<PathSelectionData<'a>>,
    ) -> Self {
        Self {
            current,
            source: PathsSource::Test(paths),
        }
    }

    /// The path currently considered the preferred path to the remote endpoint, if any.
    pub fn current(&self) -> Option<&transports::FourTuple> {
        self.current
    }

    /// Iterator over candidate paths.
    ///
    /// The same address may appear more than once when it is a path on multiple
    /// connections to the remote.  Selectors that care should aggregate as appropriate.
    pub fn paths(&self) -> Box<dyn Iterator<Item = PathSelectionData<'a>> + '_> {
        match &self.source {
            PathsSource::Live(connections) => Box::new(
                connections
                    .values()
                    .filter_map(|state| state.handle.upgrade().map(|conn| (state, conn)))
                    .flat_map(|(state, conn)| {
                        state.paths.iter().map(move |(path_id, addr)| {
                            PathSelectionData::live(addr, *path_id, conn.clone())
                        })
                    }),
            ),
            #[cfg(test)]
            PathsSource::Test(paths) => Box::new(paths.iter().cloned()),
        }
    }
}

/// Data the selector sees about one candidate path.
//
// In production this borrows from a live connection and looks up stats from noq on
// demand.  In `#[cfg(test)]` builds it can also wrap synthesized stats so selectors
// can be unit-tested without standing up real connections.
#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
#[derive(derive_more::Debug, Clone)]
pub struct PathSelectionData<'a> {
    network_path: &'a transports::FourTuple,
    #[debug(skip)]
    source: StatsSource,
}

#[derive(Clone)]
enum StatsSource {
    Live {
        path_id: PathId,
        conn: noq::Connection,
    },
    /// Boxed so `PathStats` (100+ bytes, 14 fields) doesn't inflate the enum's
    /// size in production where only the `Live` variant is ever constructed.
    #[cfg(test)]
    Test(Option<Box<PathStats>>),
}

#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
impl<'a> PathSelectionData<'a> {
    fn live(
        network_path: &'a transports::FourTuple,
        path_id: PathId,
        conn: noq::Connection,
    ) -> Self {
        Self {
            network_path,
            source: StatsSource::Live { path_id, conn },
        }
    }

    /// Constructs a [`PathSelectionData`] with synthetic stats for testing.
    ///
    /// `PathStats` is `#[non_exhaustive]` so callers build it via
    /// `let mut s = PathStats::default(); s.rtt = ...;`.
    #[cfg(test)]
    pub(crate) fn for_test(
        network_path: &'a transports::FourTuple,
        stats: Option<PathStats>,
    ) -> Self {
        Self {
            network_path,
            source: StatsSource::Test(stats.map(Box::new)),
        }
    }

    /// The network path of the candidate path.
    pub fn network_path(&self) -> &transports::FourTuple {
        self.network_path
    }

    /// Returns path statistics if available.
    pub fn stats(&self) -> Option<PathStats> {
        match &self.source {
            StatsSource::Live { path_id, conn } => conn.path_stats(*path_id),
            #[cfg(test)]
            StatsSource::Test(stats) => stats.as_deref().copied(),
        }
    }
}

/// Trait to configure path selection.
///
/// Most users do not need to provide their own selector.
#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
pub trait PathSelector: Send + Sync + std::fmt::Debug + 'static {
    /// Pick the selected path to carry application data among the currently
    /// open network paths to the remote endpoint.
    ///
    /// Build the result by starting from [`PathSelection::none`] and calling
    /// [`PathSelection::set`] for the path the selector wants active.
    ///
    /// Returning an empty [`PathSelection`] keeps the current selection unchanged.
    fn select(&self, ctx: &PathSelectionContext<'_>) -> PathSelection;
}

/// The set of paths a [`PathSelector`] has chosen.
///
/// Today this holds at most one path.  Build via [`PathSelection::none`] +
/// [`PathSelection::set`].
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
pub struct PathSelection {
    selection: Option<transports::FourTuple>,
}

#[cfg_attr(not(feature = "unstable-custom-transports"), allow(unreachable_pub))]
impl PathSelection {
    /// An empty selection.
    pub fn none() -> Self {
        Self { selection: None }
    }

    /// Sets the path as the selected path.
    ///
    /// This discards any previously selected path and sets this one as a single selected
    /// path.
    pub fn set(&mut self, path: &PathSelectionData<'_>) {
        if self.selection.is_some() {
            tracing::warn!(
                path = %path.network_path(),
                "PathSelection already contains a path; ignoring additional path"
            );
            return;
        }
        self.selection = Some(path.network_path.clone());
    }

    /// The selected path: the one data should be sent on. This is not public so
    /// we can later allow for selecting multiple paths without changing the
    /// public API of `PathSelection`.
    ///
    /// Returns `None` when nothing has been selected.
    pub(crate) fn selected(&self) -> Option<&transports::FourTuple> {
        self.selection.as_ref()
    }
}

/// Poll a future once, like n0_future::future::poll_once but sync.
fn now_or_never<T, F: Future<Output = T>>(fut: F) -> Option<T> {
    let fut = std::pin::pin!(fut);
    match fut.poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        Poll::Ready(res) => Some(res),
        Poll::Pending => None,
    }
}

/// Future that resolves to the `conn_id` once a connection is closed.
///
/// This uses [`noq::Connection::on_closed`], which does not keep the connection alive
/// while awaiting the future.
struct OnClosed {
    conn_id: ConnId,
    inner: noq::OnClosed,
}

impl OnClosed {
    fn new(conn: &noq::Connection) -> Self {
        Self {
            conn_id: ConnId(conn.stable_id()),
            inner: conn.on_closed(),
        }
    }
}

impl Future for OnClosed {
    type Output = (ConnId, Closed);

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let closed = std::task::ready!(Pin::new(&mut self.inner).poll(cx));
        Poll::Ready((self.conn_id, closed))
    }
}

/// Converts an iterator of [`TransportAddr'] into an iterator of [`transports::Addr`].
fn to_transports_addr(
    endpoint_id: EndpointId,
    addrs: impl IntoIterator<Item = TransportAddr>,
) -> impl Iterator<Item = transports::Addr> {
    addrs.into_iter().filter_map(move |addr| match addr {
        TransportAddr::Relay(relay_url) => Some(transports::Addr::from((relay_url, endpoint_id))),
        TransportAddr::Ip(sockaddr) => Some(transports::Addr::from(sockaddr)),
        TransportAddr::Custom(custom_addr) => Some(transports::Addr::from(custom_addr)),
        _ => {
            warn!(?addr, "Unsupported TransportAddr");
            None
        }
    })
}

/// Returns the next item if `maybe_stream` is `Some`, or `None` otherwise.
async fn maybe_next<S: Stream + Unpin>(maybe_stream: Option<&mut S>) -> Option<Option<S::Item>> {
    match maybe_stream {
        None => None,
        Some(s) => Some(s.next().await),
    }
}
