# Hey Chat

A sovereign, peer-to-peer messenger capsule for the Elastos Runtime. Hey Chat is
a Rust + [Leptos](https://leptos.dev) single-page app compiled to WebAssembly
(client-side rendering) and served by the runtime. It does its own end-to-end
encryption and talks directly to other people over the runtime's provider plane —
no central message server.

The whole chat engine lives in the shared **`hey-core`** rlib (compiled *into*
this app, and into `hey-social`). This crate is the messenger UI plus the
per-capsule context wiring; the crypto, transport, DM/group state machines, and
attachment handling are all `hey-core`.

## What it does

- **1-to-1 DMs** with hybrid post-quantum end-to-end encryption: ML-KEM-768 +
  X25519 (PQXDH) key agreement, a Double Ratchet for forward secrecy /
  post-compromise security where the contact has completed a handshake (and a
  single-shot fallback before then), ChaCha20-Poly1305 payloads. Delivery is
  **sealed-sender** over per-pair queues, so the relay sees only ciphertext and
  not who is talking to whom. The conversation header shows the live protection
  level (`post-quantum` vs `post-quantum + Double Ratchet`).
- **Groups** — create a group from your contacts, add members, send text and
  files. Each group message is sealed **per member** (no shared group key), so
  every link keeps its own forward secrecy.
- **Attachments** — pick any file (or multiple). Images are compressed
  client-side before upload (the pack ships no transcoder; this keeps photos
  under the runtime's 2 MB provider body limit). Small files ride **inline**
  inside the sealed message over Carrier (fragmented by `hey-core`'s
  `api/frag.rs`, no IPFS); larger files go through the content store / iroh-blobs
  and are fetched + decrypted on the receiver, shown inline (images) or as a
  download chip. Each attachment view has its own loading / retry state.
- **Invite-based contacts** — add a contact by minting a one-time `hey-invite:`
  link and exchanging it (the invite carries the X25519 + ML-KEM keys a bare
  `did:key` can't). Per-contact **Anonymous (incognito)** mode presents a
  throwaway identity. Pending invites can be revoked; conversations can be
  deleted per-device (erasing messages, files, ratchet state, and queues for
  that contact locally).
- **Connection badge** — a live pill (`ConnBadge`) polls the carrier's peer
  paths and shows whether your links are **🔒 Direct P2P** or **↪ via Relay**
  (end-to-end encrypted either way — the relay only forwards ciphertext when NAT
  blocks a direct path). A second sidebar status footer surfaces carrier
  online / connecting / offline and the outbox backlog.
- **Network / P2P settings** — advanced knobs on the shared runtime peer node:
  independent (direct, no-relay) mode, a fixed UDP port, the public address
  advertised in your shareable node ticket.
- **Link phone** — a rotating QR (`heyapp://connect?…`, self-expiring) the Hey
  phone app scans to inherit this device's wallet session with no password.

## Authentication

**Wallet-only / runtime-native SSO.** Identity comes *only* from the Elastos
runtime — either the identity provider (`identity/whoami`: a provider-backed
`did:key` with no local seed; the runtime signs and decrypts) or an inherited
runtime session (wallet SSO via Home's launch token). There is **no passkey and
no local seed** in the capsule: without the runtime there is no signing key in
the browser, so the app stays gated behind `SignInGate` / `RuntimeGate` ("no
runtime, no app"). A seed never touches `localStorage`.

## Transport

All delivery rides the Elastos Runtime **provider plane** (no in-app servers):

| Concern | Provider |
| --- | --- |
| Text / DM / group delivery | Carrier gossip (`peer` provider), iroh 1.0-rc |
| Large-file content by CID | content provider (IPFS / kubo) |
| Direct-P2P large files | `blobs` provider (iroh-blobs) |
| Identity / key resolve | `identity` / `did` providers |

Boot capabilities requested by this capsule: `elastos://peer/*` (message),
`elastos://blobs/*` (write), `elastos://did/*` (read).

## Layout

```
hey-chat/
├── capsule.json     # Elastos app capsule manifest (role: app, entrypoint index.html)
├── Cargo.toml       # crate deps; the engine is the hey-core path dependency
├── Trunk.toml       # Trunk build config (target index.html, dist/, public_url ./)
├── index.html       # Trunk entry (Trunk injects the wasm/JS)
├── styles.css       # messenger styles
├── src/
│   ├── main.rs      # mount + per-capsule CapsuleCtx (namespace, session keys, boot caps)
│   ├── lib.rs       # the Leptos app: shell, chat list, DM + group conversations,
│   │                #   composer, attachments, invite/group/network modals, ConnBadge
│   └── media.rs     # client-side image compression before upload
└── dist/            # PREBUILT output committed to the repo (wasm + JS + index.html + css)
```

There is **no** `providers/` or `client/` directory — this is one WASM app, not a
multi-process workspace.

## Build

```sh
trunk build --release
```

Run from this directory (not `cargo build` at the workspace root — the crate is
wasm32-only and kept out of the workspace default-members). Output lands in
`dist/`, which is **committed** so the runtime ships a prebuilt bundle. The
runtime serves capsule files from the mount root, and the app derives its router
base from the iframe mount path (e.g. `/apps/hey-chat/`).

## Status

Working and deployed. Hey Chat ships in the Hey capsule pack alongside
`hey-social`, sharing the `hey-core` engine.
