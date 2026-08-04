//! This module contains the implementation of the gossiping protocol for an individual topic

use std::collections::VecDeque;

use bytes::Bytes;
use derive_more::From;
use n0_future::time::{Duration, Instant};
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{
    hyparview::{self, InEvent as SwarmIn},
    plumtree::{self, GossipEvent, InEvent as GossipIn, Scope},
    state::MessageKind,
    PeerData, PeerIdentity, DEFAULT_MAX_MESSAGE_SIZE,
};
use crate::proto::MIN_MAX_MESSAGE_SIZE;

/// Input event to the topic state handler.
#[derive(Clone, Debug)]
pub enum InEvent<PI> {
    /// Message received from the network.
    RecvMessage(PI, Message<PI>),
    /// Execute a command from the application.
    Command(Command<PI>),
    /// Trigger a previously scheduled timer.
    TimerExpired(Timer<PI>),
    /// Peer disconnected on the network level.
    PeerDisconnected(PI),
    /// Update the opaque peer data about yourself.
    UpdatePeerData(PeerData),
}

/// An output event from the state handler.
#[derive(Debug, PartialEq, Eq)]
pub enum OutEvent<PI> {
    /// Send a message on the network
    SendMessage(PI, Message<PI>),
    /// Emit an event to the application.
    EmitEvent(Event<PI>),
    /// Schedule a timer. The runtime is responsible for sending an [InEvent::TimerExpired]
    /// after the duration.
    ScheduleTimer(Duration, Timer<PI>),
    /// Close the connection to a peer on the network level.
    DisconnectPeer(PI),
    /// Emitted when new [`PeerData`] was received for a peer.
    PeerData(PI, PeerData),
}

impl<PI> From<hyparview::OutEvent<PI>> for OutEvent<PI> {
    fn from(event: hyparview::OutEvent<PI>) -> Self {
        use hyparview::OutEvent::*;
        match event {
            SendMessage(to, message) => Self::SendMessage(to, message.into()),
            ScheduleTimer(delay, timer) => Self::ScheduleTimer(delay, timer.into()),
            DisconnectPeer(peer) => Self::DisconnectPeer(peer),
            EmitEvent(event) => Self::EmitEvent(event.into()),
            PeerData(peer, data) => Self::PeerData(peer, data),
        }
    }
}

impl<PI> From<plumtree::OutEvent<PI>> for OutEvent<PI> {
    fn from(event: plumtree::OutEvent<PI>) -> Self {
        use plumtree::OutEvent::*;
        match event {
            SendMessage(to, message) => Self::SendMessage(to, message.into()),
            ScheduleTimer(delay, timer) => Self::ScheduleTimer(delay, timer.into()),
            EmitEvent(event) => Self::EmitEvent(event.into()),
        }
    }
}

/// A trait for a concrete type to push `OutEvent`s to.
///
/// The implementation is generic over this trait, which allows the upper layer to supply a
/// container of their choice for `OutEvent`s emitted from the protocol state.
pub trait IO<PI: Clone> {
    /// Push an event in the IO container
    fn push(&mut self, event: impl Into<OutEvent<PI>>);

    /// Push all events from an iterator into the IO container
    fn push_from_iter(&mut self, iter: impl IntoIterator<Item = impl Into<OutEvent<PI>>>) {
        for event in iter.into_iter() {
            self.push(event);
        }
    }
}

/// A protocol message for a particular topic
#[derive(From, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Message<PI> {
    /// A message of the swarm membership layer
    Swarm(hyparview::Message<PI>),
    /// A message of the gossip broadcast layer
    Gossip(plumtree::Message),
}

impl<PI> Message<PI> {
    /// Get the kind of this message
    pub fn kind(&self) -> MessageKind {
        match self {
            Message::Swarm(_) => MessageKind::Control,
            Message::Gossip(message) => match message {
                plumtree::Message::Gossip(_) => MessageKind::Data,
                _ => MessageKind::Control,
            },
        }
    }

    /// Returns `true` if this is a disconnect message (which is the last message sent to a peer per topic).
    pub fn is_disconnect(&self) -> bool {
        matches!(self, Message::Swarm(hyparview::Message::Disconnect(_)))
    }
}

/// An event to be emitted to the application for a particular topic.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Event<PI> {
    /// We have a new, direct neighbor in the swarm membership layer for this topic
    NeighborUp(PI),
    /// We dropped direct neighbor in the swarm membership layer for this topic
    NeighborDown(PI),
    /// A gossip message was received for this topic
    Received(GossipEvent<PI>),
}

impl<PI> From<hyparview::Event<PI>> for Event<PI> {
    fn from(value: hyparview::Event<PI>) -> Self {
        match value {
            hyparview::Event::NeighborUp(peer) => Self::NeighborUp(peer),
            hyparview::Event::NeighborDown(peer) => Self::NeighborDown(peer),
        }
    }
}

impl<PI> From<plumtree::Event<PI>> for Event<PI> {
    fn from(value: plumtree::Event<PI>) -> Self {
        match value {
            plumtree::Event::Received(event) => Self::Received(event),
        }
    }
}

/// A timer to be registered for a particular topic.
///
/// This should be treated as an opaque value by the implementer and, once emitted, simply returned
/// to the protocol through [`InEvent::TimerExpired`].
#[derive(Clone, From, Debug, PartialEq, Eq)]
pub enum Timer<PI> {
    /// A timer for the swarm layer
    Swarm(hyparview::Timer<PI>),
    /// A timer for the gossip layer
    Gossip(plumtree::Timer),
}

/// A command to the protocol state for a particular topic.
#[derive(Clone, derive_more::Debug)]
pub enum Command<PI> {
    /// Join this topic and connect to peers.
    ///
    /// If the list of peers is empty, will prepare the state and accept incoming join requests,
    /// but only become operational after the first join request by another peer.
    Join(Vec<PI>),
    /// Broadcast a message for this topic.
    Broadcast(#[debug("<{}b>", _0.len())] Bytes, Scope),
    /// Leave this topic and drop all state.
    Quit,
}

impl<PI: Clone> IO<PI> for VecDeque<OutEvent<PI>> {
    fn push(&mut self, event: impl Into<OutEvent<PI>>) {
        self.push_back(event.into())
    }
}

/// Protocol configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Configuration for the swarm membership layer
    pub membership: hyparview::Config,
    /// Configuration for the gossip broadcast layer
    pub broadcast: plumtree::Config,
    /// Max message size in bytes.
    ///
    /// This size should be the same across a network to ensure all nodes can transmit and read large messages.
    ///
    /// At minimum, this size should be large enough to send gossip control messages. This can vary, depending on the size of the [`PeerIdentity`] you use and the size of the [`PeerData`] you transmit in your messages.
    ///
    /// The default is [`DEFAULT_MAX_MESSAGE_SIZE`].
    pub max_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            membership: Default::default(),
            broadcast: Default::default(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }
}

/// The topic state maintains the swarm membership and broadcast tree for a particular topic.
#[derive(Debug)]
pub struct State<PI, R> {
    me: PI,
    pub(crate) swarm: hyparview::State<PI, R>,
    pub(crate) gossip: plumtree::State<PI>,
    outbox: VecDeque<OutEvent<PI>>,
    stats: Stats,
}

impl<PI: PeerIdentity> State<PI, rand::rngs::ThreadRng> {
    /// Initialize the local state with the default random number generator.
    ///
    /// ## Panics
    ///
    /// Panics if [`Config::max_message_size`] is below [`MIN_MAX_MESSAGE_SIZE`].
    pub fn new(me: PI, me_data: Option<PeerData>, config: Config) -> Self {
        Self::with_rng(me, me_data, config, rand::rng())
    }
}

impl<PI, R> State<PI, R> {
    /// The address of your local endpoint.
    pub fn endpoint(&self) -> &PI {
        &self.me
    }
}

impl<PI: PeerIdentity, R: Rng> State<PI, R> {
    /// Initialize the local state with a custom random number generator.
    ///
    /// ## Panics
    ///
    /// Panics if [`Config::max_message_size`] is below [`MIN_MAX_MESSAGE_SIZE`].
    pub fn with_rng(me: PI, me_data: Option<PeerData>, config: Config, rng: R) -> Self {
        assert!(
            config.max_message_size >= MIN_MAX_MESSAGE_SIZE,
            "max_message_size must be at least {MIN_MAX_MESSAGE_SIZE}"
        );
        let max_payload_size =
            config.max_message_size - super::Message::<PI>::postcard_header_size();
        Self {
            swarm: hyparview::State::new(me, me_data, config.membership, rng),
            gossip: plumtree::State::new(me, config.broadcast, max_payload_size),
            me,
            outbox: VecDeque::new(),
            stats: Stats::default(),
        }
    }

    /// Handle an incoming event.
    ///
    /// Returns an iterator of outgoing events that must be processed by the application.
    pub fn handle(
        &mut self,
        event: InEvent<PI>,
        now: Instant,
    ) -> impl Iterator<Item = OutEvent<PI>> + '_ {
        let io = &mut self.outbox;
        // Process the event, store out events in outbox.
        match event {
            InEvent::Command(command) => match command {
                Command::Join(peers) => {
                    for peer in peers {
                        self.swarm.handle(SwarmIn::RequestJoin(peer), io);
                    }
                }
                Command::Broadcast(data, scope) => {
                    // Hey patch (EAGER-SET RECONCILE — the "neighbour present, nothing
                    // delivered" wedge).
                    //
                    // "Neighbour" and "will receive my broadcast" are TWO DIFFERENT
                    // SETS in this protocol, and nothing reconciles them:
                    //
                    //   * `swarm.active_view` is HyParView MEMBERSHIP. It is what emits
                    //     `NeighborUp`/`NeighborDown`, so it is the only thing the
                    //     application (and our carrier's `neighbors` map, and every
                    //     `list_topic_peers` / `has_topic_peer` check) can see.
                    //   * `gossip.eager_push_peers` is the Plumtree BROADCAST TREE. It
                    //     is what `eager_push` actually iterates. It is invisible.
                    //
                    // A peer leaves the eager set — while staying a neighbour — the
                    // instant it sends us `Message::Prune`, and it sends `Prune` from
                    // `on_gossip` for ANY message whose `MessageId` (= blake3 of the
                    // content) it has seen inside `message_id_retention` (90 s). On a
                    // 2-peer topic, which is every 1:1 DM/call lane, that single Prune
                    // empties the eager set OUTRIGHT. From then on `broadcast()` still
                    // returns `Ok`, `NeighborUp` still stands, `has_topic_peer` is still
                    // true — and `eager_push` sends the message to NOBODY. Delivery
                    // falls back to a lazy `IHave` announcement that the peer must
                    // `Graft` back; there is no retry if that announcement is lost, and
                    // the payload is only pullable while it survives the sender's 30 s
                    // `message_cache_retention`. Lose one `IHave` and the message is
                    // gone with every layer reporting success. Only a Graft, a fresh
                    // NeighborUp, or a process restart rebuilds the tree — which is
                    // exactly the observed "wedged for 45 minutes, drained on restart".
                    //
                    // Upstream has no repair for this because Plumtree assumes a swarm
                    // wide enough that some other eager edge survives a prune. At n=2
                    // that assumption does not hold.
                    //
                    // So: if we are about to broadcast with an EMPTY eager set while we
                    // do have live neighbours, re-eager them first. This is precisely
                    // the state transition a `Graft` would produce one round trip later
                    // (`on_graft` calls `add_eager`), taken now instead of after a
                    // round trip we might not survive.
                    //
                    // Why this is safe and quiet:
                    //   * it can only ADD delivery — `add_eager` moves a peer from lazy
                    //     to eager and emits no OutEvents at all;
                    //   * it fires ONLY when the eager set is empty, i.e. only when the
                    //     broadcast would otherwise reach nobody. In a real swarm with
                    //     any surviving eager edge it never runs;
                    //   * it is bounded by the active view (≤ `active_view_capacity`),
                    //     costs at most one extra message per peer per broadcast, and
                    //     adds ZERO idle traffic: it is on the publish path only, never
                    //     on a timer.
                    if self.gossip.eager_push_peers.is_empty() && !self.swarm.active_view.is_empty()
                    {
                        for peer in self.swarm.active_view.iter().copied().collect::<Vec<_>>() {
                            self.gossip.handle(GossipIn::NeighborUp(peer), now, io);
                        }
                    }
                    self.gossip
                        .handle(GossipIn::Broadcast(data, scope), now, io)
                }
                Command::Quit => self.swarm.handle(SwarmIn::Quit, io),
            },
            InEvent::RecvMessage(from, message) => {
                self.stats.messages_received += 1;
                match message {
                    Message::Swarm(message) => {
                        self.swarm.handle(SwarmIn::RecvMessage(from, message), io)
                    }
                    Message::Gossip(message) => {
                        self.gossip
                            .handle(GossipIn::RecvMessage(from, message), now, io)
                    }
                }
            }
            InEvent::TimerExpired(timer) => match timer {
                Timer::Swarm(timer) => self.swarm.handle(SwarmIn::TimerExpired(timer), io),
                Timer::Gossip(timer) => self.gossip.handle(GossipIn::TimerExpired(timer), now, io),
            },
            InEvent::PeerDisconnected(peer) => {
                self.swarm.handle(SwarmIn::PeerDisconnected(peer), io);
                self.gossip.handle(GossipIn::NeighborDown(peer), now, io);
            }
            InEvent::UpdatePeerData(data) => self.swarm.handle(SwarmIn::UpdatePeerData(data), io),
        }

        // Forward NeighborUp and NeighborDown events from hyparview to plumtree
        let mut io = VecDeque::new();
        for event in self.outbox.iter() {
            match event {
                OutEvent::EmitEvent(Event::NeighborUp(peer)) => {
                    self.gossip
                        .handle(GossipIn::NeighborUp(*peer), now, &mut io)
                }
                OutEvent::EmitEvent(Event::NeighborDown(peer)) => {
                    self.gossip
                        .handle(GossipIn::NeighborDown(*peer), now, &mut io)
                }
                _ => {}
            }
        }
        // Note that this is a no-op because plumtree::handle(NeighborUp | NeighborDown)
        // above does not emit any OutEvents.
        self.outbox.extend(io.drain(..));

        // Update sent message counter
        self.stats.messages_sent += self
            .outbox
            .iter()
            .filter(|event| matches!(event, OutEvent::SendMessage(_, _)))
            .count();

        self.outbox.drain(..)
    }

    /// Get stats on how many messages were sent and received.
    // TODO: Remove/replace with metrics?
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Reset all statistics.
    pub fn reset_stats(&mut self) {
        self.gossip.stats = Default::default();
        self.swarm.stats = Default::default();
        self.stats = Default::default();
    }

    /// Get statistics for the gossip broadcast state
    ///
    /// TODO: Remove/replace with metrics?
    pub fn gossip_stats(&self) -> &plumtree::Stats {
        self.gossip.stats()
    }

    /// Check if this topic has any active (connected) peers.
    pub fn has_active_peers(&self) -> bool {
        !self.swarm.active_view.is_empty()
    }
}

/// Statistics for the protocol state of a topic
#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// Number of messages sent
    pub messages_sent: usize,
    /// Number of messages received
    pub messages_received: usize,
}

#[cfg(test)]
mod hey_wedge_test {
    //! Hey patch regression test: the 2-peer "neighbour present, nothing delivered"
    //! wedge, reproduced with no network at all (the protocol is an IO-less state
    //! machine, so this is exact, not a simulation).

    use bytes::Bytes;
    use n0_future::time::Instant;
    use rand::SeedableRng;

    use super::*;
    use crate::proto::plumtree::Scope;

    type Node = State<u32, rand::rngs::StdRng>;

    fn node(me: u32, seed: u64) -> Node {
        State::with_rng(
            me,
            Some(PeerData::default()),
            Config::default(),
            rand::rngs::StdRng::seed_from_u64(seed),
        )
    }

    /// Drive one event into `at` and shuttle every resulting `SendMessage` between
    /// the two nodes until the pair goes quiet. Returns the messages `a` sent to `b`
    /// as a direct result of the FIRST step only — which is what we assert on.
    fn step(
        a: &mut Node,
        b: &mut Node,
        (a_id, b_id): (u32, u32),
        ev: InEvent<u32>,
        now: Instant,
    ) -> Vec<Message<u32>> {
        let first: Vec<(u32, Message<u32>)> = a
            .handle(ev, now)
            .filter_map(|e| match e {
                OutEvent::SendMessage(to, m) => Some((to, m)),
                _ => None,
            })
            .collect();
        let direct: Vec<Message<u32>> = first.iter().map(|(_, m)| m.clone()).collect();
        // Settle: deliver back and forth until neither side has anything to say.
        let mut queue: Vec<(u32, u32, Message<u32>)> =
            first.into_iter().map(|(to, m)| (a_id, to, m)).collect();
        for _ in 0..64 {
            if queue.is_empty() {
                break;
            }
            let mut next = Vec::new();
            for (from, to, m) in queue.drain(..) {
                let dst = if to == a_id { &mut *a } else { &mut *b };
                let dst_id = to;
                for e in dst.handle(InEvent::RecvMessage(from, m), now) {
                    if let OutEvent::SendMessage(t, m) = e {
                        next.push((dst_id, t, m));
                    }
                }
            }
            queue = next;
        }
        let _ = b_id;
        direct
    }

    fn is_gossip_payload(m: &Message<u32>) -> bool {
        matches!(
            m,
            Message::Gossip(crate::proto::plumtree::Message::Gossip(_))
        )
    }

    /// THE BUG, and the fix for it.
    ///
    /// Sequence, all of it real traffic this app generates:
    ///   1. A and B are neighbours on a 1:1 DM topic (2 peers, no third path).
    ///   2. A broadcasts a message. B delivers it. Normal.
    ///   3. A RETRANSMITS the same bytes — which is exactly what `outbox::flush`
    ///      does at `ACK_WAIT_MS` (45 s) for an unacked DM, and what the call-offer
    ///      nudge does every 1.2 s for 9 s. Plumtree addresses messages by
    ///      `blake3(content)` and remembers ids for 90 s, so B sees a duplicate:
    ///      it drops it AND replies `Prune`.
    ///   4. `on_prune` moves B out of A's `eager_push_peers`. With only one peer in
    ///      the topic, A's eager set is now EMPTY.
    ///   5. A broadcasts a BRAND NEW message.
    ///
    /// Before the fix, step 5 sent NO payload to B at all — `eager_push` iterated an
    /// empty set. `broadcast()` still returned `Ok`, B was still in `active_view`, so
    /// `NeighborUp` still stood and every health check up the stack (our carrier's
    /// `neighbors` map, `list_topic_peers`, `has_topic_peer`, `wire_ok`,
    /// `gossip_send`'s `delivered:true`) said the lane was healthy. Delivery
    /// depended entirely on a lazy `IHave` that B had to `Graft` back, with no retry
    /// if it was lost and a 30 s window before the payload left A's cache.
    ///
    /// After the fix, step 5 pushes the payload eagerly again.
    #[test]
    fn broadcast_still_reaches_a_neighbour_after_a_retransmit_prune() {
        let now = Instant::now();
        let (a_id, b_id) = (1u32, 2u32);
        let mut a = node(a_id, 1);
        let mut b = node(b_id, 2);

        // 1. Mesh them.
        step(
            &mut a,
            &mut b,
            (a_id, b_id),
            InEvent::Command(Command::Join(vec![b_id])),
            now,
        );
        assert!(a.has_active_peers(), "A must have B as a neighbour");
        assert!(b.has_active_peers(), "B must have A as a neighbour");
        assert!(
            a.gossip.eager_push_peers.contains(&b_id),
            "a fresh neighbour starts eager"
        );

        // 2. First send: delivered eagerly.
        let first: Bytes = b"hello".to_vec().into();
        let out = step(
            &mut a,
            &mut b,
            (a_id, b_id),
            InEvent::Command(Command::Broadcast(first.clone(), Scope::Swarm)),
            now,
        );
        assert!(
            out.iter().any(is_gossip_payload),
            "the first broadcast must go out eagerly"
        );

        // 3. The retransmit. Byte-identical, so B treats it as a duplicate and PRUNEs.
        step(
            &mut a,
            &mut b,
            (a_id, b_id),
            InEvent::Command(Command::Broadcast(first, Scope::Swarm)),
            now,
        );

        // 4. THE WEDGE: B is still a neighbour, but is no longer in the broadcast
        //    tree. This is the exact state in which every health signal lies.
        assert!(
            a.has_active_peers(),
            "the prune must NOT cost us the neighbour — that is what made this invisible"
        );
        assert!(
            a.gossip.lazy_push_peers.contains(&b_id),
            "the retransmit must have moved B to lazy (this is the defect being guarded)"
        );

        // 5. A new message must STILL be pushed to B.
        let second: Bytes = b"are you there".to_vec().into();
        let out = step(
            &mut a,
            &mut b,
            (a_id, b_id),
            InEvent::Command(Command::Broadcast(second, Scope::Swarm)),
            now,
        );
        assert!(
            out.iter().any(is_gossip_payload),
            "REGRESSION: a topic with a live neighbour accepted a broadcast and sent \
             the payload to nobody. This is the wedge."
        );
    }

    /// The reconcile must not disturb a HEALTHY tree: a peer that legitimately
    /// pruned us stays lazy for as long as some other peer is still eager, which is
    /// what keeps a real swarm from degenerating into a flood.
    #[test]
    fn reconcile_leaves_a_healthy_eager_set_alone() {
        let now = Instant::now();
        let mut a = node(1, 7);
        // Two neighbours; one prunes us, one stays eager.
        a.gossip.handle(GossipIn::NeighborUp(2), now, &mut a.outbox);
        a.gossip.handle(GossipIn::NeighborUp(3), now, &mut a.outbox);
        a.swarm.active_view.insert(2);
        a.swarm.active_view.insert(3);
        a.outbox.clear();
        a.gossip
            .handle(GossipIn::RecvMessage(2, plumtree::Message::Prune), now, &mut a.outbox);
        a.outbox.clear();

        let out: Vec<_> = a
            .handle(
                InEvent::Command(Command::Broadcast(b"x".to_vec().into(), Scope::Swarm)),
                now,
            )
            .collect();

        assert!(
            a.gossip.lazy_push_peers.contains(&2),
            "peer 2 pruned us and peer 3 is still eager, so 2 must STAY lazy"
        );
        let eager_targets: Vec<u32> = out
            .iter()
            .filter_map(|e| match e {
                OutEvent::SendMessage(to, m) if is_gossip_payload(m) => Some(*to),
                _ => None,
            })
            .collect();
        assert_eq!(
            eager_targets,
            vec![3],
            "the payload must go to the eager peer only"
        );
    }
}
