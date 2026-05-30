# Garlic overlay — design for Caveat 2 (network-layer node id / IP)

> Status: **DEFERRED / future track.** Not implemented. This captures the
> validated design so it can be picked up later. Estimated ~4–5 weeks for a
> shippable anonymous-DM overlay. Depends on continuous queue rotation (already
> landed — see `caveat 1`).

## The caveat

The other three "honest caveats" from the threat-model slide are closed at the
capsule level:

| Caveat | Status |
|---|---|
| Approx. attachment size | **Closed** — bucket + Padmé padding (`crypto::encrypt_attachment`). |
| v1 ratchet FS/PCS classical-only | **Closed** — hybrid PQ KEM-ratchet (`crypto::kdf_rk_hybrid`). |
| Packet moved to *some* queue id | **Partially closed** — continuous queue rotation cuts long-term linkability of a stable handle; a relay can still *timing-correlate* a retire→relight. |
| **Network-layer node id / IP** | **Open — needs this overlay.** |

Hey's transport is Elastos Carrier = `iroh_gossip` (a QUIC gossip mesh). The
gossip `node_id == device DID`, and `list_peers` exposes meshed endpoints. So
even with sealed-sender content, random per-pair queues, and rotation, a relay /
peer observes **which network node published to / subscribed to a topic**, i.e.
the participants' node ids and IPs, and can timing-correlate the two ends of a
conversation.

This is the SimpleX-grade network-metadata gap. It does **not** come for free on
gossip; it needs an overlay.

## What the overlay buys (and what it does NOT)

**Delivers: sender↔receiver UNLINKABILITY.** Messages travel through N
intermediary relays as nested ("garlic") sealed layers, so no single relay sees
both the origin and the destination of a message.

**Does NOT deliver: invisibility.** `node_id == DID` and `list_peers` still
expose that a node is *on the mesh*. Hiding the node id / IP itself — ephemeral
node keys, relay-only mode (never dialling peers directly) — is **Carrier /
runtime / transport work (a fork)**, out of scope for the pure-capsule overlay.
So the capsule layer closes *unlinkability*; full IP-invisibility needs the
transport track on top.

## Why it's pure-capsule (zero Carrier/runtime fork)

Verified against `carrier.rs`:

- gossip `message` content is **opaque** (passed verbatim) → an onion layer is
  just another `crypto::HpqEnvelope` payload.
- topics are **free-form strings** (`SHA-256 → TopicId`, no registration) → relays
  and the netDB live on app-chosen topics.
- `sender_id` / `signature` are **caller-supplied and UNVERIFIED** by Carrier →
  the app layer controls all routing identity (app-level Ed25519 stays the
  integrity check).
- per-`(topic, consumer_id)` cursors let one node be **both a router and an
  endpoint** at once.

The innermost onion layer is exactly today's `{type:"dm.v2", envelope}` wire, so
a fully-peeled message re-enters `receive_v2_wire` UNCHANGED. `provider_call`
(the gateway boundary) is untouched.

## Components

1. **`src/garlic.rs`** — `build_onion(route, payload)` / `peel_layer(env, key)`.
   Each hop is a nested `crypto::HpqEnvelope` sealed to that relay's advertised
   overlay key (padding is free per layer — see `crypto::pad_plaintext`). The
   innermost layer is the normal dm.v2 wire.
2. **netDB** — relays announce an overlay key + liveness on a well-known topic
   (`hey-garlic/netdb/v1`). The sender samples a random route from it.
3. **`consume_garlic_inbox()`** — one pass in `peer_receiver.rs`: pull from our
   garlic-inbox topic, `peel_layer`; if we're an intermediary, re-publish the
   inner layer to the next hop's inbox; if we're the endpoint, hand the peeled
   dm.v2 wire to `receive_v2_wire`.
4. **Routed-send branch** — in `dms.rs` send path, gated by a per-conversation
   flag, wrap the dm.v2 wire in an onion instead of publishing it directly.
   Pairs with the existing `IdentityMode::Anonymous` (a routed send is only
   meaningful for an anonymous contact — a Regular contact's DID is already known
   to the peer).

## Honest limits (accepted, carry verbatim)

- **Not anonymous transport** — node_id == DID; `list_peers` exposes endpoints.
  Unlinkability, not invisibility (see "transport track" above).
- **Latency** — store-and-forward, poll-based; tens of seconds for ~3 hops.
- **Lossy** — gossip drops messages → needs an app-level ACK + resend.
- **WASM relays route only while the app is open** → real coverage needs an
  always-on relay (the one thing that would justify a separate relay provider
  capsule). Until then, anonymity sets are small.
- **Needs scale** — a small mesh is a weak anonymity set.
- **Sybil / route-capture is the central UNSOLVED hardening problem** — free
  ephemeral relay keys let an adversary flood the netDB and bias route
  selection. Mitigations (stake, reputation, trusted-intro routes) are open
  research; ship with eyes open.

## Sequencing

1. Continuous queue rotation — **done** (the overlay routes between rotating
   queues; together they deliver the timing-decorrelation rotation alone can't).
2. `garlic.rs` primitives + `self_test` (build/peel N layers, round-trip).
3. netDB announce/sample + `consume_garlic_inbox()` in the poll loop.
4. Routed-send branch gated per-conversation, paired with Anonymous mode.
5. ACK + resend; relay liveness; route diversity.
6. (Separate transport track, fork) ephemeral node keys + relay-only mode for
   node-id/IP invisibility.
