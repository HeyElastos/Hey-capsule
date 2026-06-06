# hey-core

The shared chat **engine** for the Hey capsule pack — the security-critical core
compiled into both **hey-social** and **hey-chat** so the two apps stay
byte-identical and interoperate on one chat network.

`hey-core` is a plain **`rlib`** (default crate-type, *not* `cdylib`). It has **no
`capsule.json`** — it is not a standalone capsule. The wasm bin crates pull it in
as a path dependency:

```toml
hey-core = { path = "../hey-core" }
```

…and Trunk compiles the final app wasm. Keeping a single copy of the crypto, DM,
and transport code means the audit surface is one file tree and the wire format
can never drift between the two apps. (See `src/lib.rs` for the module map.)

The crate is wasm32-first (it backs the browser apps via `getrandom('js')` +
`web-sys`) but `plat.rs` is cfg-split so the pure engine also builds for a native
target — the `hey-chat-cli` / `hey-social-cli` test harnesses link it on std.

## What it provides

- **Hybrid post-quantum E2E encryption** for direct messages.
- **Sealed-sender, metadata-safe DMs** over per-pair random queues.
- **Double Ratchet** key schedule (FS + post-compromise security) layered on the
  PQXDH-style hybrid floor.
- **Groups** with a self-carrying roster (TOFU-via-creator trust).
- **End-to-end-encrypted attachments** with three transport paths (inline /
  content-store / iroh-blobs).
- A **provider-call transport abstraction** over the Elastos Runtime HTTP plane.
- **Key-continuity pinning** (TOFU + `key_verified`) to surface MITM / key
  rotation instead of silently re-trusting.

## Security model (`crypto.rs`)

Every DM is sealed with a **hybrid** construction — an attacker must break **both**
primitives to recover plaintext:

```
shared_secret = HKDF-SHA256( X25519_dh || ML-KEM-768_secret , info = "hey-messenger/hpq-1" )
ciphertext    = ChaCha20-Poly1305( padded_plaintext, key = shared_secret, nonce )
```

- **ML-KEM-768** (NIST FIPS 203, RustCrypto `ml-kem`) is the post-quantum KEM;
  **X25519** is the classical half. Same hybrid pattern as Signal PQXDH.
- Identity is **`did:key` + Ed25519** (`identity.rs`), produced byte-identically
  to the old JS reference for cross-capsule identity continuity. The user's
  X25519 keypair is derived from the same 32-byte Ed25519 seed; the ML-KEM-768
  keypair is generated once at first sign-in and persisted with the session.
- **Metadata hardening:** the `hpq-2` envelope length-prefixes and zero-pads the
  plaintext up to a fixed **size bucket** (`PAD_BUCKETS = 256 / 1024 / 4096 /
  16384 / 65536`, then +64 KiB) so the ciphertext length leaks only the bucket,
  not the true message size. `hpq-1` (raw) envelopes still **decrypt** for
  back-compat; we only **encrypt** to `hpq-2`.

### Double Ratchet

`crypto.rs` holds the pure key-schedule primitives (`root_init`, `kdf_rk`,
`kdf_rk_hybrid`, `kdf_ck`, `encrypt_with_mk`, `open_with_secrets`); the state
machine that drives them lives in `api/dms.rs` (`dm/ratchet/<did>.json`).

- Classical X25519 + the DH ratchet always deliver **forward secrecy** and
  **post-compromise security**.
- `kdf_rk_hybrid` folds a **fresh per-turn ML-KEM secret** (from a rolling KEM
  keypair rotated each turn) into the root KDF, so for contacts bootstrapped
  after the hybrid upgrade, **PCS is post-quantum** (recovery after an unobserved
  turn needs breaking both X25519 and ML-KEM-768). Pre-upgrade contacts stay
  classical-only via plain `kdf_rk`.
- Skipped-message keys are bounded (`MAX_SKIP = 1000` per advance,
  `MAX_SKIPPED_KEYS = 2000`, 7-day TTL); a forged jump counter is rejected
  *before* any KDF runs. `ratchet_capable` is sticky — a contact can't be
  silently downgraded back to the no-PCS single-shot path.

## Sealed-sender DMs (`api/dms.rs`)

DMs default to **v2 — metadata-safe per-pair queues**:

- Each contact has a private random **256-bit queue id**; the wire topic is
  `q/<queue_id>` with **no DID in the topic name**, so the `peer` provider sees
  only opaque queue traffic between random pseudonyms (SimpleX-style).
- The envelope is **sealed-sender**: `{sender_did, signature, text}` all live
  *inside* the ChaCha20-Poly1305 ciphertext. The provider sees only
  `{ "type": "dm.v2", "envelope": HpqEnvelope }`.
- The first contact between strangers is bootstrapped from an **out-of-band
  invite link** (24 h TTL, single-use queue retired after the handshake) — no
  plaintext is ever sent on the wire.
- **Queue rotation:** each side periodically rotates its own inbound queue
  (after `QUEUE_ROTATE_MSGS` / `QUEUE_ROTATE_MS`) so the relay can't build a long
  history against one durable handle, with a grace window so in-flight messages
  aren't lost.

v1 (DID-in-topic) is still **received** for legacy contacts but new contacts
always use v2.

### Key-continuity pinning (TOFU + `key_verified`)

Encryption keys are **trust-on-first-use**. Once a contact has pinned keys,
`bootstrap_contact_from_keys` **never silently replaces them with different
keys**:

- `key_verified = true` ⇒ keys are **self-asserted** (invite handshake, a signed
  follow/friend link, or a direct key confirmation).
- `key_verified = false` ⇒ keys are **vouched by a third party** (a group roster:
  TOFU-via-creator) — pinned but **unverified**.
- A self-assertion **upgrades** a prior unverified pin (unverified → verified).
  Any *other* key mismatch is **refused and logged** as a possible MITM or key
  rotation — surfaced, never auto-trusted. A roster assertion never overwrites
  verified keys.

## Groups

A `Group` carries its full **roster** (`GroupMember { did, name, peer_pubkeys,
peer_ticket }`) in every message, so a member who never saw an explicit invite
still materialises the group on first receipt and can bootstrap a pairwise
channel to members it doesn't yet know. Trust is **TOFU-via-creator** (the
creator vouches for the keys; an existing *verified* contact is never overwritten
with roster keys). Groups have an owner (`created_by`), optional bio/avatar,
join-consent (`pending`), and pinned messages.

## Attachments (three paths)

Each attachment is sealed under its **own fresh random ChaCha20-Poly1305 key**;
only the ciphertext leaves the device and the per-file key rides **inside** the
sealed DM, so the store/relay only ever holds opaque bytes. The sealed plaintext
is bucket-padded (message ladder ≤ 64 KiB, then **Padmé** with ≤ ~11–12 % overhead
for large files) so stored size reveals only a bucket. `upload_attachment` picks
a path by size/availability:

1. **Inline over the Carrier** — files **≤ 16 KB** (`INLINE_ATTACHMENT_MAX_BYTES`):
   the sealed ciphertext is base64'd into the DM body (`Attachment.inline_b64`)
   and rides the Carrier (fragmented like any oversized wire by `api/frag.rs`).
   **No IPFS round-trip** — instant and unaffected by content-provider health.
2. **iroh-blobs direct P2P** — preferred for large files: the ciphertext is
   chunked and added to the `blobs` provider, one **ticket** per chunk
   (`Attachment.tickets`). The recipient fetches each ticket **directly P2P** from
   the holder — no IPFS add/pin/DHT. *Liveness tradeoff:* the holder must be
   **online** when the recipient fetches (no relay/pin cushion).
3. **Content store (IPFS CID)** — the **fallback** and offline-capable path
   (pinned + federated): ciphertext is split into ≤ 1 MiB chunks (each under the
   runtime's 2 MB provider body limit) and published via `content/publish`,
   recording the chunk CIDs (`Attachment.cid` / `Attachment.chunks`).

The blobs attempt is **all-or-nothing**: if the provider is unavailable or any
chunk fails, the whole upload transparently falls through to the content store —
so today, before the blobs provider is registered, every large upload lands on
the content path. `fetch_attachment` reverses whichever path was used (inline →
tickets → cid/chunks), concatenates, decrypts, and size-checks.

## Transport (`runtime.rs` + `api/outbox.rs` + `api/frag.rs`)

All I/O goes through one boundary — `runtime::provider_call(scheme, op, body)`
POSTs to the runtime's `POST /api/provider/<scheme>/<op>`, with typed wrappers:

- **`peer`** — Carrier gossip (`gossip_join` / `gossip_send` / `gossip_recv`,
  tickets, neighbor checks) for text / DM / group delivery. iroh 1.0-rc carrier.
- **`content`** — IPFS/kubo by CID (`content/publish`, `content/fetch`).
- **`blobs`** — iroh-blobs direct-P2P large-file tickets.
- **`storage`** — per-capsule namespaced JSON store; **`identity` /
  `did`** providers (provider-backed signing / DH / decapsulation) are wired too.

Per-capsule identity (capsule id, storage namespace, storage keys, boot
capability wants-list) is **injected** by the bin crate via
`hey_core::ctx::init(CapsuleCtx { .. })` in `main()` — none of hey-social's values
are baked into the shared crate.

Reliability layers on top of the providers:

- **`api/frag.rs`** — iroh-gossip silently drops messages over its
  `max_message_size` (~4096 B). The PQ invite **handshake** runs ~23 KB, so it
  never crossed (the long-hunted "send ok / recv empty" bug). `frag` splits an
  oversized wire into ordered fragments, tags each with a shared id, and
  reassembles on receive *before* `dms::receive_v2_wire`. Small wires pass through
  byte-for-byte unchanged.
- **`api/outbox.rs`** — every publish that isn't **confirmed delivered** (sent,
  not a `local_only` 0-neighbor no-op, and a topic neighbor actually exists) is
  stashed to `dm/outbox.json` with exponential backoff and retried each
  `peer_receiver` poll cycle (re-grafting the gossip mesh before each retry).
- **`peer_receiver.rs`** — the background poll loop: subscribes to the v2 per-pair
  queues, gates on an inbound neighbor before draining, routes decrypted DMs into
  the store, flushes the outbox, and exposes pluggable handlers + extra topics so
  hey-social can re-add its feed/group routing (hey-chat registers none → DM-only).

## Auth

**Wallet-only / runtime-native SSO.** The runtime / Home owns identity; there is
**no passkey and no local seed** managed by the capsule. `session.rs` persists
only `{ did_key, name, ml_kem_secret_b64, ml_kem_public_b64 }` to detect "am I
signed in?" and to seed the crypto; `wipe_identity()` drops it for shared-machine
workflows.

## Build

`hey-core` is built transitively when you build either app with Trunk:

```bash
# from capsules/hey-social or capsules/hey-chat
trunk build --release
```

It is deliberately excluded from the workspace `default-members`, so a host-target
`cargo build` at the repo root does not try to build the wasm-only crate. The
native CLI harnesses build it for their own target via cfg-split `plat.rs`. A
quick crypto sanity check is `crypto::self_test()` (round-trips the hybrid seal,
the ratchet primitives, and attachment padding).

## Status

**Working, deployed engine** — not a scaffold. It backs the live hey-social and
hey-chat apps (PQ-E2E DMs, groups, and attachments over the Carrier). The known
soft spot is cross-host **transport liveness** (carrier neighbor formation /
content-provider fetch can flap at the runtime layer), not the engine's crypto or
data model.

> Not implemented (do **not** assume): WebRTC voice/video/screen-share calls,
> iroh-docs CRDT workspaces, Teams-style channels/workspaces. `transcoder` /
> `elacity` / IPLD-dag-cbor wrappers are stubbed/unported in `runtime.rs`.
