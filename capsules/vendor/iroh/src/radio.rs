//! HYPER PATCH (additive): one aligned keepalive tick, so the cellular modem can sleep.
//!
//! # The problem this solves
//!
//! On a mobile radio, what costs battery is not *how much* you send, it is *how often you
//! wake the modem*. After the last packet an LTE/NR modem stays in `RRC_CONNECTED` for an
//! inactivity tail (order 5-10s) before it drops to a cheap idle state. Any packet inside
//! that tail restarts it.
//!
//! iroh has four unrelated periodic senders, and none of them divides any other:
//!
//! | source | period | where |
//! |---|---|---|
//! | QUIC keepalive PING, applied **per path** | 5s | [`crate::socket::HEARTBEAT_INTERVAL`] |
//! | relay websocket ping | 15s | `socket::transports::relay::actor::PING_INTERVAL` |
//! | `net_report` re-STUN | 20-26s (random) | `socket::new_re_stun_timer` |
//! | QAD address discovery keepalive | 25s | `iroh_relay::quic` (not vendored) |
//!
//! Four free-running timers at 5/15/~23/25s means *something* fires almost continuously.
//! The 5s one alone is fatal: it is shorter than the inactivity tail, so on its own it
//! pins the modem in `RRC_CONNECTED` forever. Measured on the owner's handset: 100%
//! cellular radio residency across a 6h30m unplugged, screen-off window.
//!
//! # The fix
//!
//! Lengthening one interval is nearly worthless while the others stay unaligned — you
//! still pay a wake per timer. **Alignment is the lever.** So:
//!
//! * every periodic sender snaps its deadline onto one process-wide grid derived from a
//!   single [`epoch`] and a single [`period`], so they all fire on the *same instant*
//!   instead of at four independent phases;
//! * noq's autonomous per-path keepalive is switched off and replaced by one tick driven
//!   from the per-remote actor, which pings every open path at once;
//! * the period is posture-dependent: [`ACTIVE_PERIOD`] while the app is foreground or a
//!   call is up (latency matters and the radio is already awake), [`IDLE_PERIOD`]
//!   otherwise.
//!
//! [`ACTIVE_PERIOD`] divides [`IDLE_PERIOD`] exactly, on purpose: both grids share the
//! epoch, so every idle slot is also an active slot and a posture flip cannot introduce a
//! beat frequency between the two.
//!
//! # Safety rails
//!
//! Sending less often means a lost keepalive costs more, and a dead QUIC connection would
//! push message delivery onto the slow periodic sweep — a far worse outcome than any
//! battery win. Two mitigations are therefore part of the design, not optional extras:
//!
//! 1. the connection idle timeout is raised to [`ALIGNED_MAX_IDLE_TIMEOUT`], and the
//!    per-path idle timeout to [`ALIGNED_PATH_MAX_IDLE_TIMEOUT`], so a tick may be lost
//!    several times over before anything is torn down. Per RFC 9000 §10.1 the effective
//!    idle timeout is `min(ours, peer's)`, so against a peer that has not been upgraded
//!    this degrades to exactly today's behaviour rather than breaking;
//! 2. each tick sends **two** packets [`BURST_GAP`] apart, so a single drop does not cost
//!    the whole interval.
//!
//! # This is off by default
//!
//! [`aligned_enabled`] is `false` until something calls [`set_aligned_enabled`]. With the
//! flag off every code path in this module is a predictable-branch no-op and iroh behaves
//! byte-for-byte as upstream. Nothing here changes default behaviour.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        OnceLock,
    },
    time::Duration,
};

use n0_future::time::Instant;

/// Keepalive period while the app is foreground or a call is active.
///
/// Matches the upstream `HEARTBEAT_INTERVAL`. In this posture the radio is already up and
/// being paid for, so there is nothing to save and latency is what matters.
pub const ACTIVE_PERIOD: Duration = Duration::from_secs(5);

/// Keepalive period while the app is backgrounded and no call is active.
///
/// Chosen as an exact multiple of [`ACTIVE_PERIOD`] so the two grids nest (see the module
/// docs), and comfortably longer than the RRC inactivity tail so the modem actually gets
/// to drop to idle between ticks.
pub const IDLE_PERIOD: Duration = Duration::from_secs(15);

/// Gap between the two packets of one tick burst.
///
/// Long enough that the second packet is a genuinely independent transmission opportunity,
/// short enough that both land inside one radio wake and so cost nothing extra.
pub const BURST_GAP: Duration = Duration::from_millis(300);

/// Connection-level idle timeout used while the aligned tick is on.
///
/// Four [`IDLE_PERIOD`] ticks. The negotiated value is `min(ours, peer's)` (RFC 9000
/// §10.1), so a peer still running the old build simply holds this down to its own 30s and
/// nothing breaks.
pub const ALIGNED_MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-path idle timeout used while the aligned tick is on.
///
/// Three [`IDLE_PERIOD`] ticks — two whole ticks (four packets) may be lost before a path
/// is abandoned. Kept below [`ALIGNED_MAX_IDLE_TIMEOUT`] so a broken path is still dropped
/// (and traffic fails over to another path) before the whole connection is.
pub const ALIGNED_PATH_MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// Master flag. `false` = upstream behaviour, everywhere.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Holepunch-backoff flag — a SEPARATE switch from the aligned tick.
///
/// Deliberately independent so the two can be rolled out, measured and reverted one at a
/// time. They address different things: the tick changes *when* we transmit, this changes
/// *how often we manufacture new connections and paths*.
static HOLEPUNCH_BACKOFF: AtomicBool = AtomicBool::new(false);

/// Base delay before repeating a holepunch round that found no new candidates.
///
/// Same value as upstream's `HOLEPUNCH_ATTEMPTS_INTERVAL`, so the first repeat is timed
/// exactly as before and only *sustained* fruitless retrying is slowed.
pub const HOLEPUNCH_BACKOFF_BASE: Duration = Duration::from_secs(5);

/// Ceiling for the holepunch backoff.
///
/// We never stop retrying entirely — NAT state can change without any candidate changing,
/// so a blind retry is not worthless, just nearly so. Five minutes keeps that possibility
/// alive at negligible cost.
pub const HOLEPUNCH_BACKOFF_MAX: Duration = Duration::from_secs(300);

/// Turns the holepunch backoff on or off for this process.
pub fn set_holepunch_backoff_enabled(on: bool) {
    HOLEPUNCH_BACKOFF.store(on, Ordering::Relaxed);
}

/// Whether the holepunch backoff is on.
#[inline]
pub fn holepunch_backoff_enabled() -> bool {
    HOLEPUNCH_BACKOFF.load(Ordering::Relaxed)
}

/// The delay before a holepunch round may repeat with an unchanged candidate set.
///
/// `rounds` is how many times in a row we have already re-punched without learning a single
/// new address. Doubles from [`HOLEPUNCH_BACKOFF_BASE`] to [`HOLEPUNCH_BACKOFF_MAX`].
///
/// The safety argument for backing off at all: with an unchanged candidate set we are
/// re-dialling exactly the addresses that already failed. Every event that could plausibly
/// change the outcome — a new candidate, a link change, a path coming up, the last IP path
/// going down — resets `rounds` to zero at the call site, so this only ever slows down
/// retries that have no new information behind them.
pub fn holepunch_backoff(rounds: u32) -> Duration {
    let base = HOLEPUNCH_BACKOFF_BASE.as_secs();
    let max = HOLEPUNCH_BACKOFF_MAX.as_secs();
    // Cap the shift before it is applied so the doubling cannot overflow on a long-lived
    // actor that has been idle for days.
    let shift = rounds.min(16);
    let secs = base.saturating_mul(1u64 << shift).min(max);
    Duration::from_secs(secs)
}

/// Adaptive path-lifecycle flag — a THIRD switch, independent of the other two.
///
/// Independent on purpose, for the same reason the holepunch backoff is: it must be possible
/// to roll this out, measure it and revert it without disturbing anything else. The backoff
/// changes *how often we manufacture new paths*; this changes *how long we keep the ones we
/// already have*.
static PATH_LIFECYCLE: AtomicBool = AtomicBool::new(false);

/// How long a path must have been out of service before the lifecycle sweep will retire it.
///
/// Upstream never retires on idleness at all — `prune_non_relay_paths` bails out unless the
/// path table is already AT `MAX_NON_RELAY_PATHS`, so a path that stops working is only ever
/// evicted to make room for a newer one. Steady state is therefore "hold the maximum
/// forever, mostly dead", which is exactly what the path census showed on hardware.
///
/// Five minutes is deliberately long: path knowledge is what makes a reconnect fast, so the
/// cost of retiring too early (a slower reconnect) is real, while the cost of retiring too
/// late is only a slightly larger table. It also matches [`HOLEPUNCH_BACKOFF_MAX`], so a
/// peer we have given up holepunching to is retired on the same clock we stop dialling it.
pub const PATH_RETIRE_IDLE_AFTER: Duration = Duration::from_secs(300);

/// How many out-of-service paths to keep as warm spares, however old they are.
///
/// This is the "plus a small spare" half of "converge on the working path plus a small
/// spare". It is also the hard floor that makes it impossible for the sweep to strip a
/// remote bare: with a non-zero spare count there is always something left to dial even if
/// every single path has aged out and there is no relay path.
pub const PATH_RETIRE_SPARES: usize = 2;

/// How long the sweep stands down after a real network-change signal.
///
/// Re-widening must never be a blind timer — it is driven by the signals that actually mean
/// "what we know about reachability just became stale": a link change, a new candidate, a
/// direct path coming up, or losing the last direct path. This is the *length* of that
/// stand-down, not a schedule: nothing arms it except one of those events.
///
/// Two minutes is two `UPGRADE_INTERVAL` rounds, so a change gets at least one full
/// holepunch-and-validate cycle to widen the path set back out before narrowing can resume.
///
/// [`UPGRADE_INTERVAL`]: crate::socket::remote_map
pub const PATH_WIDEN_HOLD: Duration = Duration::from_secs(120);

/// Turns the adaptive path lifecycle on or off for this process.
pub fn set_path_lifecycle_enabled(on: bool) {
    PATH_LIFECYCLE.store(on, Ordering::Relaxed);
}

/// Whether the adaptive path lifecycle is on.
#[inline]
pub fn path_lifecycle_enabled() -> bool {
    PATH_LIFECYCLE.load(Ordering::Relaxed)
}

/// Count of paths retired by the lifecycle sweep, for the runtime snapshot.
static PATHS_RETIRED: AtomicUsize = AtomicUsize::new(0);

/// Count of lifecycle sweeps that actually retired something.
static PATH_SWEEPS: AtomicUsize = AtomicUsize::new(0);

/// Records that a lifecycle sweep retired `n` paths.
pub(crate) fn record_paths_retired(n: usize) {
    PATHS_RETIRED.fetch_add(n, Ordering::Relaxed);
    PATH_SWEEPS.fetch_add(1, Ordering::Relaxed);
}

/// Lifecycle counters for the runtime snapshot: `(paths_retired, sweeps_that_retired)`.
///
/// Monotonic since process start. Zero while the flag is on and the path census is not
/// falling means the sweep is not reaching anything — check the widening hold first, since
/// a peer whose candidates keep changing legitimately never narrows.
pub fn path_lifecycle_counters() -> (usize, usize) {
    (
        PATHS_RETIRED.load(Ordering::Relaxed),
        PATH_SWEEPS.load(Ordering::Relaxed),
    )
}

/// Speculative fan-out budget — a FOURTH switch, independent of the other three.
///
/// # The defect
///
/// When a remote has no selected path, `handle_msg_send_datagram` sends the datagram to
/// **every address in the path table**. That table is capped at `MAX_NON_RELAY_PATHS`
/// (30) and, on a CGNAT handset, it saturates at exactly that: 30 private candidates
/// harvested from tickets and address lookup, none of them reachable, none of them ever
/// removed (upstream's `prune_non_relay_paths` only evicts under cap pressure, and an
/// untried candidate never acquires a prunable status).
///
/// So every QUIC connect to that remote costs `30 destinations x 8 packets` — the Initial
/// plus its PTO ladder at t=0,1,1,3,3,6,6,10s inside the 10s connect timeout — 240
/// datagrams of ~1200 B, about 288 KB, for a connect that only ever completes over the
/// relay. On device that measured as 3.649 pkt/s and 66% of all transmitted bytes at idle.
///
/// # The fix, and why it is adaptive rather than a smaller constant
///
/// A fixed "send to at most K" would be a different wrong number: it would pick K entries
/// out of an `FxHashMap` whose iteration order is per-process seeded, so it could
/// deterministically exclude the one candidate that works, for the whole life of the
/// process. That is a delivery bug, and delivery outranks every efficiency win.
///
/// Instead each candidate gets a **budget**: [`FANOUT_PROBE_BUDGET`] speculative datagrams
/// — one complete connect ladder — to prove it can answer. Spend it and the candidate goes
/// cold, and cold candidates are no longer paid for on every burst; they are retried a few
/// at a time on a rotating cursor, so every one of them is still re-probed within a bounded
/// number of bursts. Nothing is ever abandoned, and the budget is re-armed by real signals:
/// a path coming up on that address (proof of life) re-arms that address, and a link change
/// re-arms every address, because the link moving is the one event that makes all of our
/// reachability knowledge stale at once.
///
/// The result is that the cost of a connect stops scaling with the number of dead
/// candidates: 30 dead candidates cost the same as 3. That is the property the owner asked
/// for — more contacts must mean *less* traffic per contact, never more.
///
/// # This is off by default
///
/// [`fanout_budget_enabled`] is `false` until something calls
/// [`set_fanout_budget_enabled`]. With the flag off the selector returns the full path
/// table and iroh behaves exactly as upstream.
static FANOUT_BUDGET: AtomicBool = AtomicBool::new(false);

/// Speculative datagrams a candidate gets to prove it can answer, before it goes cold.
///
/// Eight is not a taste call: it is exactly one QUIC connect ladder. noq sends the Initial
/// and then PTO-retransmits it at t=0,1,1,3,3,6,6,10s within the 10s connect timeout, so
/// eight datagrams is one complete, fair trial of a candidate — the same trial upstream
/// gives it. What changes is only what happens on the *second* connect to the same dead
/// address, and the hundredth.
///
/// Counting datagrams rather than bursts is deliberate: a burst boundary is not observable
/// from inside the send path, and a candidate that ate eight packets without a reply has
/// had its trial however those packets were grouped.
pub const FANOUT_PROBE_BUDGET: u32 = 8;

/// How many cold candidates to re-probe per speculative send, when a relay path exists.
///
/// The relay is the delivery backstop and it is always included, so cold IP candidates here
/// are pure opportunism: they buy a direct path, never delivery itself. Two per datagram
/// means one connect ladder sweeps 16 candidates and two ladders sweep a full saturated
/// table, which bounds re-probe latency at roughly one extra connect round while cutting a
/// 30-destination burst to 3.
///
/// Must be >= 1. Zero would turn "narrow the fan-out" into "abandon every candidate", and
/// a peer that came back on the LAN would never be found again without a link change.
pub const FANOUT_COLD_RETRIES: usize = 2;

/// How many cold candidates to re-probe per speculative send when there is **no** relay or
/// custom path to this remote.
///
/// This is the case the fan-out loop genuinely owns: an mDNS-discovered LAN peer, or a
/// ticket whose relay was filtered out, has no backstop at all, and this loop is the only
/// way a first packet ever reaches it. So we widen instead of narrowing — eight per
/// datagram sweeps a full 30-entry table inside a single connect ladder, i.e. first contact
/// is no slower than upstream. It is still 8 destinations rather than 30, and it only
/// applies to remotes we have no other route to.
pub const FANOUT_COLD_RETRIES_NO_RELAY: usize = 8;

/// Turns the speculative fan-out budget on or off for this process.
pub fn set_fanout_budget_enabled(on: bool) {
    FANOUT_BUDGET.store(on, Ordering::Relaxed);
}

/// Whether the speculative fan-out budget is on.
#[inline]
pub fn fanout_budget_enabled() -> bool {
    FANOUT_BUDGET.load(Ordering::Relaxed)
}

/// Datagram-destinations actually sent to by the speculative fan-out.
static FANOUT_SENT: AtomicUsize = AtomicUsize::new(0);

/// Datagram-destinations the budget declined to send to.
static FANOUT_SKIPPED: AtomicUsize = AtomicUsize::new(0);

/// Records one speculative fan-out: `sent` destinations used, `skipped` declined.
pub(crate) fn record_fanout(sent: usize, skipped: usize) {
    FANOUT_SENT.fetch_add(sent, Ordering::Relaxed);
    FANOUT_SKIPPED.fetch_add(skipped, Ordering::Relaxed);
}

/// Fan-out counters for the runtime snapshot: `(destinations_sent, destinations_skipped)`.
///
/// Monotonic since process start. `skipped` is the whole point — it is the packet count
/// this switch removed, measured rather than modelled. `skipped == 0` while the flag is on
/// means either no remote has a saturated candidate table (fine) or the selector is never
/// reached because every remote has a selected path (also fine, and worth knowing before
/// blaming the switch for a residency number that did not move).
pub fn fanout_counters() -> (usize, usize) {
    (
        FANOUT_SENT.load(Ordering::Relaxed),
        FANOUT_SKIPPED.load(Ordering::Relaxed),
    )
}

/// Address-store freshness — bounding the accumulator that feeds everything above.
///
/// # The defect
///
/// [`MemoryLookup`] is where out-of-band addressing information lands, and it is a pure
/// accumulator: [`MemoryLookup::add_endpoint_info`] unions the incoming addresses into
/// whatever is already stored, there is no TTL, and its own docs say entries "are only
/// removed explicitly" — which nothing ever does. Every address a peer has *ever* been
/// advertised at stays resolvable forever.
///
/// That is not a leak of memory, it is a leak of *transmissions*. The stored set is
/// returned in full on every dial, is inserted into the path table as
/// `PathStatus::Unknown`, and `Unknown` is precisely the status the path lifecycle sweep
/// refuses to retire (untried candidates are dial hints). So the address store is upstream
/// of every bound we have, and it is unbounded.
///
/// Measured on the handset: eight entries for one peer, same IPv4 host, eight different UDP
/// ports — one per time that peer's socket was rebound — each drawing an equal share of the
/// speculative fan-out. Seven of the eight could not possibly answer.
///
/// The growth term is `contacts x rebinds`, so it is worst for exactly the accounts that
/// matter most: old ones with many contacts.
///
/// # The rule
///
/// A peer's *own re-advertisement is the freshness signal*, and it already arrives — pkarr
/// republishes, the app re-asserts stored tickets on every re-dial cycle, and live paths
/// re-insert themselves. So an address that has stopped being advertised is an address the
/// peer has stopped claiming, and the timestamps to detect that are already being written;
/// nothing reads them. This switch reads them.
///
/// On every insert, per peer:
///
/// * **Non-IP addresses are kept unconditionally and never counted.** The relay URL is the
///   delivery backstop that every gossip broadcast and every first contact with a NAT'd peer
///   rides. Dropping it would cost messages, so it is not eligible for either rule below.
/// * **At most [`ADDR_MAX_IP`] IP addresses**, keeping the most recently advertised. This is
///   the bound, and it holds whatever the timestamps say.
/// * **IP addresses not re-advertised within [`ADDR_STALE_AFTER`] are dropped** — but only
///   when a non-IP backstop exists. A peer known *only* by IP (an mDNS LAN neighbour, a
///   ticket whose relay was filtered) keeps its addresses regardless of age, because for
///   that peer this store is the only way to reach them and ageing it out is a disconnect
///   rather than a saving.
///
/// Together those two make a peer moving IPv4 -> IPv6, or public -> behind NAT, or simply
/// restarting onto a new port, self-correcting: the new address arrives fresh, the old one
/// stops being refreshed and falls out. No protocol change, no new packet, no new timer —
/// the eviction rides an insert that was happening anyway.
///
/// # What this cannot do
///
/// It cannot drop an address the peer *keeps* advertising but that we cannot reach. That is
/// correct: it is the peer's current claim about itself, and it is the fan-out budget's job
/// to stop paying full price for it. The two switches compose — this one bounds the set,
/// [`fanout_budget_enabled`] bounds what a saturated set costs per send.
///
/// [`MemoryLookup`]: crate::address_lookup::memory::MemoryLookup
/// [`MemoryLookup::add_endpoint_info`]: crate::address_lookup::memory::MemoryLookup::add_endpoint_info
static ADDR_FRESHNESS: AtomicBool = AtomicBool::new(false);

/// Hard cap on stored IP addresses per peer. Non-IP (relay, custom) addresses are exempt.
///
/// Sized from what a genuinely multi-homed peer can hold at once — a LAN IPv4, a
/// STUN-mapped IPv4, a global IPv6, a temporary IPv6, plus one spare. Anything beyond that
/// is history rather than reachability. This is what makes the store `O(contacts)` instead
/// of `O(contacts x rebinds)`.
pub const ADDR_MAX_IP: usize = 6;

/// How long an IP address survives in the store without being re-advertised.
///
/// Must comfortably exceed the slowest thing that re-advertises, or a live address would be
/// dropped between refreshes. The slowest is pkarr republication at five minutes, so this is
/// six missed republications. The cap above is the real bound; this only trims what is both
/// stale *and* under the cap, and it never applies to a peer with no non-IP backstop.
pub const ADDR_STALE_AFTER: Duration = Duration::from_secs(1800);

/// Turns address-store freshness on or off for this process.
pub fn set_addr_freshness_enabled(on: bool) {
    ADDR_FRESHNESS.store(on, Ordering::Relaxed);
}

/// Whether address-store freshness is on.
#[inline]
pub fn addr_freshness_enabled() -> bool {
    ADDR_FRESHNESS.load(Ordering::Relaxed)
}

/// IP addresses evicted from the address store as stale.
static ADDRS_EVICTED_STALE: AtomicUsize = AtomicUsize::new(0);

/// IP addresses evicted from the address store by the per-peer cap.
static ADDRS_EVICTED_CAP: AtomicUsize = AtomicUsize::new(0);

/// Records an eviction pass: `stale` aged out, `capped` dropped by [`ADDR_MAX_IP`].
pub(crate) fn record_addrs_evicted(stale: usize, capped: usize) {
    ADDRS_EVICTED_STALE.fetch_add(stale, Ordering::Relaxed);
    ADDRS_EVICTED_CAP.fetch_add(capped, Ordering::Relaxed);
}

/// Address-store counters for the runtime snapshot: `(evicted_stale, evicted_by_cap)`.
///
/// Monotonic since process start. Both zero while the flag is on is the *expected* reading
/// on a young account with few contacts — it means nothing had accumulated yet, not that the
/// switch failed to arm. `evicted_by_cap` climbing steadily while `evicted_stale` stays at
/// zero means addresses are being re-advertised faster than [`ADDR_STALE_AFTER`] and the cap
/// is doing all the work, which is the intended division of labour.
pub fn addr_store_counters() -> (usize, usize) {
    (
        ADDRS_EVICTED_STALE.load(Ordering::Relaxed),
        ADDRS_EVICTED_CAP.load(Ordering::Relaxed),
    )
}

/// Posture provider, registered by the embedder.
///
/// Returns `true` when the device is in an "active" posture (app foreground or call up).
/// A fn pointer rather than a boxed closure so reading it is a plain relaxed load on the
/// hot path.
static POSTURE: OnceLock<fn() -> bool> = OnceLock::new();

/// The grid origin, latched on first use.
///
/// `Instant` has no portable absolute representation, so the grid is anchored on the first
/// instant anyone asks about it. Every snapped deadline in the process is computed from
/// this one origin, which is precisely what makes the sources land on the same tick.
static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Count of aligned keepalive bursts actually emitted, for the runtime snapshot.
static BURSTS: AtomicUsize = AtomicUsize::new(0);

/// Count of individual path pings emitted by the aligned tick.
static PINGS: AtomicUsize = AtomicUsize::new(0);

/// Turns the aligned keepalive tick on or off for this process.
///
/// Call it before building the [`Endpoint`], because the connection-level QUIC keepalive
/// and idle timeout can only be chosen when the transport config is built. The per-path
/// settings and every snapped timer re-read the flag live, so flipping it later still
/// takes effect for those — it just cannot retroactively rewrite an already-negotiated
/// connection.
///
/// [`Endpoint`]: crate::Endpoint
pub fn set_aligned_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether the aligned keepalive tick is on.
#[inline]
pub fn aligned_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Registers the posture provider.
///
/// The provider must return `true` when the app is foreground or a call is active. It is
/// polled from timer paths only (never per packet), so it may take a lock, but it should
/// not block. Only the first registration wins.
pub fn set_posture_provider(f: fn() -> bool) {
    let _ = POSTURE.set(f);
}

/// Whether we are in the low-latency posture (app foreground or call active).
///
/// Defaults to `true` when no provider is registered, so a host that never wires posture
/// up keeps the short, delivery-safe period rather than silently getting the long one.
#[inline]
pub fn active_posture() -> bool {
    match POSTURE.get() {
        Some(f) => f(),
        None => true,
    }
}

/// The current tick period for the posture we are in.
#[inline]
pub fn period() -> Duration {
    if active_posture() {
        ACTIVE_PERIOD
    } else {
        IDLE_PERIOD
    }
}

/// The grid origin, latching it on first call.
fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

/// The first grid slot strictly after `t`.
///
/// This is the whole trick: two callers with unrelated timers that both snap through here
/// end up waiting for the *same* instant, so they cost one radio wake between them instead
/// of two.
pub fn snap_after(t: Instant) -> Instant {
    let epoch = epoch();
    let period = period().as_nanos().max(1);
    // `t` can precede the epoch only in the first instants of the process; treat that as
    // slot zero rather than doing signed arithmetic on `Instant`.
    let elapsed = t.checked_duration_since(epoch).unwrap_or_default().as_nanos();
    // `+ 1` makes it strictly after, which is what a timer wants: snapping an instant that
    // already sits exactly on a slot must yield the next slot, not a zero-length sleep.
    let offset = (elapsed / period + 1).saturating_mul(period);
    epoch + Duration::from_nanos(u64::try_from(offset).unwrap_or(u64::MAX))
}

/// The next grid slot from now — the deadline for a keepalive tick.
pub fn next_tick() -> Instant {
    snap_after(Instant::now())
}

/// Snaps a would-be deadline `now + base` up onto the grid.
///
/// Used by the periodic senders that are not the keepalive itself (relay ping, re-STUN).
/// They keep their own idea of how often they want to run; all this does is defer each run
/// to the next moment the radio is going to be awake anyway. The wait therefore lands
/// somewhere in `[base, base + period)`.
pub fn snap_interval(base: Duration) -> Instant {
    snap_after(Instant::now() + base)
}

/// Records that a keepalive burst went out, with the number of paths pinged.
pub(crate) fn record_burst(pings: usize) {
    BURSTS.fetch_add(1, Ordering::Relaxed);
    PINGS.fetch_add(pings, Ordering::Relaxed);
}

/// Aligned-tick counters for the runtime snapshot: `(bursts, path_pings)`.
///
/// Monotonic since process start. A burst count that is not advancing at roughly
/// `uptime / period` while the flag is on means the tick is not running, which is the
/// first thing to check if residency does not move.
pub fn counters() -> (usize, usize) {
    (
        BURSTS.load(Ordering::Relaxed),
        PINGS.load(Ordering::Relaxed),
    )
}

// ---------------------------------------------------------------------------
// QAD address-discovery connection reuse.
// ---------------------------------------------------------------------------

/// QAD-reuse flag — its OWN switch, independent of every other one in this module.
///
/// # The defect
///
/// `net_report` learns our public address by opening a QUIC connection to a relay with the
/// `/iroh-qad/0` ALPN and reading the `OBSERVED_ADDRESS` frame the server sends back. The
/// connection is deliberately long-lived — it carries a 25s QUIC keepalive and a 35s idle
/// timeout — and `net_report` keeps it in `QadConns` precisely so the *next* re-STUN cycle
/// can read the current address off it instead of dialling again.
///
/// That reuse never happens. `run_probe_v4` awaits the first observed address, builds the
/// report from it, and then creates the connection's observer as
/// `Watchable::new(None)` — it throws the value it just measured away. The background task
/// that feeds the observer is driven by `noq::ObservedExternalAddr`, which is a
/// `tokio::sync::watch` stream and therefore only ever yields **on a change**. On a stable
/// NAT mapping the address never changes, so nothing is ever written and the observer stays
/// `None` for the life of the connection.
///
/// `QadConns::current_v4` returns `None` whenever the observer is `None`, so
/// `needs_v4_probe` (`v4_report.is_none()`) is permanently `true`, and every re-STUN
/// cycle — one per 20-26s, forever, screen off or on — opens a fresh QUIC+TLS handshake to
/// every relay in the map. The retained connection is already `Some`, so all of those new
/// connections are closed the instant they finish: their entire contribution is the
/// handshake itself.
///
/// Measured on the owner's handset in an 896s window: 1015 datagrams to the four relay QAD
/// addresses (mean 557 B) and 136 `/iroh-qad/0` handshakes, against 29 completed reports.
/// That is ~1.13 pkt/s of transmit, and on the receive side a full server certificate chain
/// per handshake — the largest single named idle cost on the device, spent entirely on
/// re-learning an address we already knew.
///
/// # The fix
///
/// Seed the observer with the report that was just measured, so the retained connection is
/// *recognisably usable* and the predicate that already exists upstream can do its job.
/// This is subtraction only: the connection, its keepalive and its idle timeout are
/// unchanged, and no new timer, packet or state is introduced.
///
/// # Why this is adaptive rather than a smaller constant
///
/// Nothing here is a schedule. The cached answer is used for exactly as long as it keeps
/// proving itself, and three *real signals* — not a timer — re-arm the full probe:
///
/// * the address actually changes: the relay sends a new `OBSERVED_ADDRESS`, the background
///   task writes it to the observer, and the next cycle reads the new value. A CGNAT rebind
///   is therefore still discovered, by the very mechanism this bug had disabled;
/// * the connection dies: a rebind or a dead path trips the 35s idle timeout, `close_reason`
///   becomes `Some`, `spawn_qad_probes` drops the entry and re-probes from scratch;
/// * the network changes, or five minutes elapse: `do_full` clears `QadConns` outright and
///   re-probes every relay, which is also what keeps home-relay selection multi-relay.
///
/// It also scales the right way: the saving is per *relay per cycle* and is completely
/// independent of how many contacts, followers or topics the node has.
static QAD_REUSE: AtomicBool = AtomicBool::new(false);

/// Count of QAD QUIC handshakes started, across both address families.
static QAD_HANDSHAKES: AtomicUsize = AtomicUsize::new(0);

/// Count of net_report cycles that reached the QAD stage at all.
static QAD_CYCLES: AtomicUsize = AtomicUsize::new(0);

/// Count of those cycles served entirely from already-open connections.
static QAD_REUSED: AtomicUsize = AtomicUsize::new(0);

/// Turns QAD connection reuse on or off for this process.
///
/// Read live on every cycle, so it can be flipped at any time; it only affects connections
/// opened after the flip, which converge within one [`FULL_REPORT_INTERVAL`].
///
/// [`FULL_REPORT_INTERVAL`]: crate::net_report
pub fn set_qad_reuse_enabled(on: bool) {
    QAD_REUSE.store(on, Ordering::Relaxed);
}

/// Whether QAD connection reuse is on.
#[inline]
pub fn qad_reuse_enabled() -> bool {
    QAD_REUSE.load(Ordering::Relaxed)
}

/// Let the relay SERVER drive the keepalive cadence instead of pre-empting it.
///
/// # The defect
///
/// The relay server already pings every client. `iroh-relay`'s `PING_INTERVAL` is 15s plus
/// 1-5s of jitter, and — this is the part that matters — its timer is reset on EVERY frame
/// it receives from us:
///
/// ```text
/// // reset the ping interval, we just received a message
/// ping_interval.reset();
/// ```
///
/// Our own client ping fires every 15s, which is strictly shorter, so it resets that timer
/// before it can ever elapse. The server's ping therefore never fires, and the cadence on
/// the wire is OURS, not theirs. We were paying for a keepalive that the far end was already
/// willing to pay for.
///
/// # Why the gap is the whole point
///
/// Radio residency is decided by the GAP between transmissions, not by how many packets are
/// in each one: the modem sleeps roughly 10s after the last packet. A 15s gap gives about
/// 10/15 = 67% residency. Letting the server drive at 16-20s gives about 10/18 = 56%. Same
/// number of exchanges, ~11 points of residency, purely from not going first.
///
/// # Why this is flag-gated rather than just a longer constant
///
/// The 15s was chosen as half of a 30s idle timeout, explicitly so a single lost ping still
/// leaves a chance to recover. Backing off leans on the server actually pinging us — which
/// its own source guarantees, but which is an assumption about someone else's deployment.
/// Ours stays armed as a BACKSTOP at [`SERVER_DRIVEN_PING_INTERVAL`] rather than being
/// deleted, so a server that goes quiet is still detected, just later.
///
/// It also scales the right way: the saving is per relay connection and is completely
/// independent of contacts, followers or topics.
static SERVER_DRIVEN_PING: AtomicBool = AtomicBool::new(false);

/// Our backstop ping period when the server is driving. Comfortably longer than the
/// server's 16-20s so it normally never fires, short enough that a silent server is still
/// noticed within a minute.
/// Chosen to be longer than the server ping on a relay WE run, which is the whole point.
///
/// The server resets its ping timer on every frame it receives from us, so whichever side
/// ticks faster sets the cadence on the wire. At 45s this backstop still beat a 90s
/// self-hosted relay and the gap stayed 45s — the relay patch would have been deployed,
/// measured, and wrongly written off. At 120s the server always goes first: 16-20s on n0's
/// public relays, 90s on ours.
///
/// Residency follows the gap, because the modem sleeps ~10s after the last packet:
/// ~10/18 = 56% on n0, ~10/90 = 11% on a 90s relay of our own.
///
/// SAFETY, VERIFIED IN TREE RATHER THAN ASSUMED. The obvious hazard is a keepalive longer
/// than something that reaps the connection. Two candidates, both checked:
///   * `RELAY_INACTIVE_CLEANUP_TIME` is 60s — SHORTER than this. But all three
///     `inactive_timeout` arms in the relay actor are gated `if !self.is_home_relay`, so the
///     home relay — the one connection that must survive for us to be reachable — is exempt
///     and cannot be reaped by inactivity. A non-home relay being cleaned up after 60s idle
///     is the intended behaviour and is unaffected.
///   * The 30s QUIC idle timeout in the original comment governs QUIC paths, not this
///     WebSocket-over-TCP relay connection.
///
/// The cost is detection latency: a server that dies silently is noticed in up to 120s
/// rather than 45s. For a phone that is the right trade — a reconnect is cheap, and holding
/// the radio awake to discover the same fact sooner is not.
pub const SERVER_DRIVEN_PING_INTERVAL: Duration = Duration::from_secs(120);

/// Turns server-driven relay keepalive on or off for this process.
pub fn set_server_driven_ping(on: bool) {
    SERVER_DRIVEN_PING.store(on, Ordering::Relaxed);
}

/// Whether the relay server is being allowed to drive the keepalive cadence.
#[inline]
pub fn server_driven_ping() -> bool {
    SERVER_DRIVEN_PING.load(Ordering::Relaxed)
}

/// Is there a local gateway worth asking for a port mapping?
///
/// `procure_mapping()` runs on EVERY net_report cycle — every 20-26s — and fires a PCP
/// packet, a NAT-PMP packet and a UPnP SSDP multicast. On a home LAN that is a reasonable
/// trade: a successful mapping opens an inbound port and buys real direct connectivity.
///
/// On cellular there is nothing to ask. A CGNAT core does not run a UPnP/PCP gateway for
/// subscribers, so all three go out and NOTHING ever answers — measured as pure send with
/// zero receive on the owner's handset. Worse, `UNAVAILABILITY_TRUST_DURATION` in the
/// portmapper crate is only 5s, far shorter than the 20-26s report cycle, so the "we already
/// know this is unavailable" cache has always expired by the next attempt and it re-probes
/// forever rather than backing off.
///
/// Keyed on the link type rather than disabled outright (`PortmapperConfig::Disabled` would
/// also work but is fixed at endpoint-build time) so a phone carried from cellular back onto
/// WiFi resumes port mapping on the very next cycle. Defaults TRUE, matching upstream, so a
/// host that never reports its link type behaves exactly as before.
static PORTMAP_ALLOWED: AtomicBool = AtomicBool::new(true);

/// Allow or suppress gateway port-mapping probes for this process.
pub fn set_portmap_allowed(on: bool) {
    PORTMAP_ALLOWED.store(on, Ordering::Relaxed);
}

/// Whether gateway port-mapping probes are worth sending on the current link.
#[inline]
pub fn portmap_allowed() -> bool {
    PORTMAP_ALLOWED.load(Ordering::Relaxed)
}

/// Thin the periodic net_report while the app is backgrounded.
///
/// # What the periodic report buys, and when it stops buying it
///
/// `re_stun(Periodic)` fires every 20-26s. It re-discovers our reflexive address and
/// re-picks the home relay. Both matter while a user is actively using the app and direct
/// paths are worth having. Backgrounded, with relay-collapse engaged, we deliberately
/// advertise no direct candidates at all — so most of that work is discovering an address
/// nobody will be told about.
///
/// It is not cheap either. A full report tears down and re-dials both QAD QUIC connections,
/// fires HTTPS probes across every relay in the map, and the resulting address change
/// cascades: `store_direct_addresses` -> a pkarr publish -> `local_direct_addrs.updated()`
/// in every remote actor -> QNT AddAddress frames to every peer -> a holepunch round each.
/// One periodic tick can therefore turn into a burst across every contact.
///
/// # Why THIN rather than SKIP
///
/// Never reporting in background would let the reflexive address and relay choice go stale
/// with nothing to notice, and would need a whole recovery path of its own. Letting one tick
/// in [`BG_NETREPORT_EVERY`] through keeps a slow heartbeat — roughly every four minutes
/// instead of every twenty-odd seconds — so staleness is still bounded by something that
/// already exists, and a genuine event (link change, a dead relay) still forces a report
/// through the non-periodic paths, which are untouched.
static BG_NETREPORT: AtomicBool = AtomicBool::new(false);

/// Let one periodic report in this many through while backgrounded. 12 x ~23s ~= 4.6 min.
pub const BG_NETREPORT_EVERY: u32 = 12;

/// Counts periodic ticks so the thinning is deterministic rather than sampled.
static BG_NETREPORT_TICKS: AtomicUsize = AtomicUsize::new(0);

/// Turn background net_report thinning on or off for this process.
pub fn set_bg_netreport(on: bool) {
    BG_NETREPORT.store(on, Ordering::Relaxed);
}

/// Should THIS periodic re-STUN tick run?
///
/// Always true when the switch is off or the app is active — so foreground behaviour is
/// byte-identical. When backgrounded, true once every [`BG_NETREPORT_EVERY`] ticks.
pub fn netreport_tick_allowed() -> bool {
    if !BG_NETREPORT.load(Ordering::Relaxed) || active_posture() {
        return true;
    }
    let n = BG_NETREPORT_TICKS.fetch_add(1, Ordering::Relaxed);
    n as u32 % BG_NETREPORT_EVERY == 0
}

/// The relay the user explicitly chose, if any — pinned as home relay when it answers.
///
/// # Why a pin is needed at all
///
/// Setting a custom relay ADDS it to the map; it does not select it. Home-relay selection
/// is a latency contest, and n0 runs a global anycast fleet, so a single VPS will usually
/// lose it. The observed result on the owner's handset was a relay map containing
/// elastos.app while the home relay was `usw1-1.relay.n0.iroh.link` — a US West relay,
/// chosen from Sweden. "My own relay" was true of the map and false of reality.
///
/// That is not merely surprising, it defeats the only lever that actually moves radio
/// residency. The relay's keepalive interval is compiled into the server, so it can only be
/// changed on a relay we operate; if we never CONNECT to that relay, patching it does
/// nothing at all. Latency is also the wrong metric to optimise here — a relay 40ms further
/// away that pings every 90s instead of every 16s is enormously better for a phone.
///
/// # Why this cannot strand anyone
///
/// The pin is honoured ONLY when the pinned relay has a measured latency in the current
/// report, i.e. it answered us this cycle. A VPS that is down, unreachable or has not
/// answered simply has no latency, the pin is skipped, and selection falls back to the
/// normal contest with n0 still in the map. So the failure mode of a self-hosted relay is
/// "we quietly use n0 until yours is back", never "the user is unreachable".
static PINNED_RELAY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Pin the home relay to the user's own, or clear it with `None`.
pub fn set_pinned_relay(url: Option<String>) {
    if let Ok(mut g) = PINNED_RELAY.lock() {
        *g = url.map(|u| u.trim().trim_end_matches('/').to_string()).filter(|u| !u.is_empty());
    }
}

/// The pinned home relay, if the user set one.
pub fn pinned_relay() -> Option<String> {
    PINNED_RELAY.lock().ok().and_then(|g| g.clone())
}

/// Does `url` refer to the pinned relay? Compared host-wise and ignoring a trailing dot or
/// slash, because a `RelayUrl` round-trips as an FQDN (`https://host./`) while the user
/// types `https://host`.
pub fn is_pinned_relay(url: &str) -> bool {
    let Some(p) = pinned_relay() else { return false };
    let norm = |s: &str| {
        s.trim()
            .trim_end_matches('/')
            .trim_end_matches('.')
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('.')
            .to_ascii_lowercase()
    };
    norm(url) == norm(&p)
}

/// The seed for a fresh QAD connection's observed-address `Watchable`.
///
/// `first` is the report built from the address the relay reported during the handshake —
/// i.e. the answer we just paid for. With the flag on it becomes the connection's initial
/// observed value, which is what makes the connection reusable. With the flag off this
/// returns `None`, reproducing upstream byte for byte, including the re-dial.
///
/// Generic so this module stays free of `net_report`'s private types, and so the property
/// that matters — "off means `None`, on means `Some`" — can be tested from outside the
/// crate, where the vendored fork's own test suite cannot run.
#[inline]
pub fn qad_seed_observed<T>(first: T) -> Option<T> {
    if qad_reuse_enabled() { Some(first) } else { None }
}

/// Records one QAD stage of one net_report cycle, and how many handshakes it started.
pub(crate) fn record_qad_cycle(handshakes: usize) {
    QAD_CYCLES.fetch_add(1, Ordering::Relaxed);
    if handshakes == 0 {
        QAD_REUSED.fetch_add(1, Ordering::Relaxed);
    } else {
        QAD_HANDSHAKES.fetch_add(handshakes, Ordering::Relaxed);
    }
}

/// QAD counters for the runtime snapshot: `(handshakes, cycles, cycles_served_from_cache)`.
///
/// Monotonic since process start. These exist because "the fix is on and working" and "the
/// fix never ran" are otherwise indistinguishable from outside the process — the same
/// ambiguity the burst counter kills for the aligned tick. Read them like this:
///
/// * `handshakes / cycles` near the relay count means reuse is NOT happening (the upstream
///   defect, or the flag is off);
/// * `cycles_served_from_cache / cycles` near `1 - restun_period / 5min` (≈0.92 at a 25s
///   re-STUN) means it is working;
/// * `cycles_served_from_cache` stuck at zero while the flag is on points at a connection
///   that keeps dying — check the relay, not this module.
pub fn qad_counters() -> (usize, usize, usize) {
    (
        QAD_HANDSHAKES.load(Ordering::Relaxed),
        QAD_CYCLES.load(Ordering::Relaxed),
        QAD_REUSED.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_is_strictly_after_and_on_grid() {
        let epoch = epoch();
        let period = period();
        // Exactly on a slot must advance to the next slot, never return a zero-length wait.
        let on_slot = epoch + period * 4;
        let snapped = snap_after(on_slot);
        assert!(snapped > on_slot);
        assert_eq!(snapped, epoch + period * 5);

        // Just past a slot rounds up to the following one.
        let just_past = epoch + period * 4 + Duration::from_millis(1);
        assert_eq!(snap_after(just_past), epoch + period * 5);
    }

    #[test]
    fn independent_callers_land_on_the_same_instant() {
        // The point of the module: two unrelated deadlines inside one period collapse onto
        // a single wake.
        let epoch = epoch();
        let period = period();
        let a = snap_after(epoch + period * 2 + Duration::from_millis(10));
        let b = snap_after(epoch + period * 2 + period / 2);
        assert_eq!(a, b);
    }

    #[test]
    fn active_and_idle_grids_nest() {
        // Every idle slot must also be an active slot, otherwise flipping posture would
        // create a beat frequency and cost extra wakes.
        assert_eq!(IDLE_PERIOD.as_nanos() % ACTIVE_PERIOD.as_nanos(), 0);
    }

    #[test]
    fn idle_timeouts_leave_room_for_lost_ticks() {
        // A path must survive at least two whole missed ticks, and the connection must
        // outlive the path so failover happens before teardown.
        assert!(ALIGNED_PATH_MAX_IDLE_TIMEOUT >= IDLE_PERIOD * 3);
        assert!(ALIGNED_MAX_IDLE_TIMEOUT > ALIGNED_PATH_MAX_IDLE_TIMEOUT);
    }

    #[test]
    fn disabled_by_default() {
        assert!(!aligned_enabled());
    }

    #[test]
    fn path_lifecycle_disabled_by_default() {
        // Every one of the four switches ships off, and each is independent of the others.
        assert!(!path_lifecycle_enabled());
        assert!(!holepunch_backoff_enabled());
        assert!(!aligned_enabled());
        assert!(!fanout_budget_enabled());
    }

    #[test]
    fn fanout_budget_disabled_by_default() {
        assert!(!fanout_budget_enabled());
    }

    #[test]
    fn fanout_always_retries_at_least_one_cold_candidate() {
        // The floor that makes "nothing is ever abandoned" structural rather than a rule
        // someone has to remember. Zero here would strand a peer that came back on the LAN
        // until the next link change.
        assert!(FANOUT_COLD_RETRIES >= 1);
        assert!(FANOUT_COLD_RETRIES_NO_RELAY >= FANOUT_COLD_RETRIES);
    }

    #[test]
    fn fanout_budget_covers_a_whole_connect_ladder() {
        // A candidate must get a complete, fair trial before it is allowed to go cold —
        // the Initial plus its seven PTO retransmits inside the 10s connect timeout.
        // Anything less would let a single dropped packet retire a live LAN peer.
        assert!(FANOUT_PROBE_BUDGET >= 8);
    }

    #[test]
    fn fanout_sweeps_a_saturated_table_without_a_relay_inside_one_ladder() {
        // With no relay there is no backstop, so first contact must not get slower: one
        // ladder's worth of datagrams must be able to reach every entry of a path table
        // saturated at the `MAX_NON_RELAY_PATHS` cap. That cap lives in a `pub(super)`
        // const two modules away, so it is restated here; `path_state`'s
        // `fanout_no_relay_window_covers_the_cap` test pins the two together.
        const MAX_NON_RELAY_PATHS: usize = 30;
        let ladder = FANOUT_PROBE_BUDGET as usize;
        assert!(ladder * FANOUT_COLD_RETRIES_NO_RELAY >= MAX_NON_RELAY_PATHS);
    }

    #[test]
    fn path_lifecycle_always_leaves_a_spare() {
        // The floor that makes "never prune the last usable path" structural rather than a
        // rule someone has to remember: with a non-zero spare count the sweep cannot empty
        // a path table even if every path has aged out and there is no relay.
        assert!(PATH_RETIRE_SPARES >= 1);
    }

    #[test]
    fn path_lifecycle_widens_before_it_narrows_again() {
        // A stand-down shorter than the idle threshold would let a path learned right after
        // a link change be retired before it has had a chance to prove itself.
        assert!(PATH_WIDEN_HOLD < PATH_RETIRE_IDLE_AFTER);
        // And it must cover at least one full holepunch-and-validate round (60s).
        assert!(PATH_WIDEN_HOLD >= Duration::from_secs(120));
    }
}
