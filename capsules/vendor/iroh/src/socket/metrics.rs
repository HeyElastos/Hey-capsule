use iroh_metrics::{Counter, MetricsGroup};
use serde::{Deserialize, Serialize};

/// Metrics collected by the iroh socket.
#[derive(Debug, Serialize, Deserialize, MetricsGroup)]
#[non_exhaustive]
#[metrics(name = "socket", default)]
pub struct Metrics {
    /// Intended to count updates to the local direct address set, but currently unused
    /// (never incremented).
    pub update_direct_addrs: Counter,

    /// Number of bytes sent over IPv4.
    pub send_ipv4: Counter,
    /// Number of bytes sent over IPv6.
    pub send_ipv6: Counter,
    /// Number of bytes sent over the relay transport.
    pub send_relay: Counter,

    /// Number of UDP datagrams sent over IPv4 (GSO segments counted individually).
    ///
    /// The `send_ipv4` counter is a BYTE count, so on its own it cannot be compared against a
    /// packet count from the OS (`/proc/net/dev`, Android `netstats`). This counts the datagrams
    /// that actually reach the wire, which is what those OS counters count.
    pub send_ipv4_datagrams: Counter,
    /// Number of UDP datagrams sent over IPv6 (GSO segments counted individually).
    pub send_ipv6_datagrams: Counter,
    /// Number of successful `poll_send` calls on an IP transport (i.e. sendmsg syscalls).
    ///
    /// Differs from the datagram counters exactly by the GSO batching factor.
    pub send_ip_calls: Counter,
    /// Number of datagrams handed to the relay transport for sending.
    ///
    /// These are carried inside the relay's TCP/WebSocket connection, so this is NOT a count of
    /// packets on the wire — but it is the count of relay sends the endpoint asked for.
    pub send_relay_datagrams: Counter,

    /// Number of data bytes received over the relay transport.
    pub recv_data_relay: Counter,
    /// Number of data bytes received over any custom transport.
    pub recv_data_custom: Counter,
    /// Number of data bytes received over IPv4.
    pub recv_data_ipv4: Counter,
    /// Number of data bytes received over IPv6.
    pub recv_data_ipv6: Counter,
    /// Number of QUIC datagrams received.
    pub recv_datagrams: Counter,
    /// Number of receive events that used GRO (coalesced datagram batches).
    ///
    /// This counts batches, not the individual datagrams within them. See
    /// [`Self::recv_datagrams`] for the datagram count.
    pub recv_gro_datagrams: Counter,

    /// Number of times the home relay changed to a different relay.
    ///
    /// This includes the initial assignment from no home relay to a home relay.
    pub relay_home_change: Counter,

    /*
     * Holepunching metrics
     */
    /// The number of times holepunching is initiated on a connection.
    ///
    /// This can be incremented multiple times for a single connection. Note that only the
    /// client-side of a connection will increment this counter.
    pub holepunch_attempts: Counter,
    /// The number of network paths to peers that are direct.
    ///
    /// This can be incremented multiple times for a single connection.
    pub paths_direct: Counter,
    /// The number of network paths to peers that are relayed.
    ///
    /// This would typically only be incremented once for a single connection.
    pub paths_relay: Counter,
    /// The number of network paths to peers that are user defined.
    ///
    /// This would typically only be incremented once for a single connection.
    pub paths_custom: Counter,
    /// The number of connections that have been direct connections.
    ///
    /// This is only incremented once for each opened connection. See `num_conns_opened` for
    /// the number of opened connections.
    pub num_conns_direct: Counter,

    /*
     * Connection Metrics
     *
     * These all only count connections that completed the TLS handshake successfully. This means
     * that short lived 0RTT connections are potentially not included in these counts.
     */
    /// Number of connections opened (only handshaked connections are counted).
    pub num_conns_opened: Counter,
    /// Number of connections closed (only handshaked connections are counted).
    pub num_conns_closed: Counter,

    /// Number of IP transport paths opened.
    pub transport_ip_paths_added: Counter,
    /// Number of IP transport paths closed.
    pub transport_ip_paths_removed: Counter,
    /// Number of relay transport paths opened.
    pub transport_relay_paths_added: Counter,
    /// Number of relay transport paths closed.
    pub transport_relay_paths_removed: Counter,
    /// Number of custom transport paths opened.
    pub transport_custom_paths_added: Counter,
    /// Number of custom transport paths closed.
    pub transport_custom_paths_removed: Counter,

    /// Number of iterations of the main socket actor loop.
    pub actor_tick_main: Counter,
    /// Number of actor messages processed by the socket actor loop.
    pub actor_tick_msg: Counter,
    /// Number of periodic re-STUN timer ticks handled by the socket actor loop.
    pub actor_tick_re_stun: Counter,
    /// Number of port-mapping change events handled by the socket actor loop.
    pub actor_tick_portmap_changed: Counter,
    /// Intended to count direct address heartbeat ticks, but currently unused (never incremented).
    pub actor_tick_direct_addr_heartbeat: Counter,
    /// Number of local network interface (link) change events handled by the socket actor loop.
    pub actor_link_change: Counter,
    /// Number of times an input watcher or receiver closed in the socket actor loop.
    pub actor_tick_other: Counter,
}
