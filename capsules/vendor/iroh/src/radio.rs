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
