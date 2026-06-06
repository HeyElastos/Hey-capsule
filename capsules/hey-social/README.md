# hey-social

A sovereign **photo / video + chat social app**, packaged as an Elastos Runtime capsule. Built in **Rust + [Leptos](https://leptos.dev) compiled to WASM** via [Trunk](https://trunkrs.dev) (this is the Rust port that replaced the earlier React app). The `dist/` build artifacts are committed so the runtime can serve the capsule without a build step.

## What it is

A web2-shaped social feed (think a photo/video timeline) layered on a decentralized, post-quantum-encrypted backbone:

- **Feed** — a photo timeline (`/`, `pages/home.rs`) and a videos/clips tab (`/videos`, `pages/clips.rs`), with post detail (`/p/:id`) and a video player (`/v/:id`).
- **Posts** — image/video upload with **client-side WebP compression** (`media.rs`: resize to ≤2048px, WebP q0.82, in the browser before upload) and a **fail-fast size guard** so an oversize body fails in milliseconds with a clear message instead of stalling on the runtime's ~2 MB provider body limit. Media is pinned to IPFS by CID. Posts also carry overlay edits (reactions, reposts, comments). See `api/posts.rs`, `pages/posts.rs`, `components/post_card.rs`.
- **Profile + follow/following** — your profile, avatar upload, and a follow graph (`api/profile.rs`: `follow_user`, `unfollow_user`, `list_following`, `list_followers`, `follow_counts`, friend/follow links). A public profile (bio + follower/following counts) is published for cross-node parity. Pages: `pages/profile.rs`, panels `components/following_panel.rs` / `components/contacts_panel.rs`.
- **Direct messages + groups** — post-quantum end-to-end encrypted 1:1 DMs (`/chat/:did`) and group chats (`/chat/g/:group_id`), with **attachment send/render**. See `pages/chat.rs`, `api/groups.rs`, and the DM re-export `api/dms.rs`.
- **Connection badge** — `components/conn_badge.rs` (`ConnBadge`, hoisted into the sticky `TopHeader`) polls the carrier and shows whether your live P2P links are **🔒 Direct P2P** (relay-free) or **↪ via Relay** (forwarded when NAT blocks a direct path). End-to-end encrypted either way — the relay sees only ciphertext.

## Architecture

### Shared engine: hey-core

The security-critical chat core is **not** in this crate. hey-social depends on **`hey-core`** (`../hey-core`, an rlib compiled *into* this app — no capsule.json of its own), so hey-social and hey-chat are **one chat network with one audit surface**. `api/dms.rs` re-exports the engine's DM module wholesale (only shimming `generate_invite` / `accept_invite` to default hey-social to the `Regular` federated identity); `crypto` and `identity` are likewise re-exported from the engine. `src/main.rs` installs hey-social's per-capsule `CapsuleCtx` (its localStorage keys, namespace `Hey`, declared boot capabilities) before mounting.

hey-core provides:

- **Hybrid post-quantum E2E**: ML-KEM-768 + X25519 PQXDH handshake, Double Ratchet (forward secrecy + post-compromise security), ChaCha20-Poly1305 sealed envelopes.
- **Sealed-sender, per-pair-queue DMs**, groups, and contacts (add-contact is invite-only — invites carry the PQ keys and the peer's iroh node ticket).
- **Attachments** (`hey-core/src/api/dms.rs`):
  - files **≤ 16 000 bytes** ride **INLINE** inside the sealed DM over the Carrier (fragmented by `api/frag.rs`, no IPFS);
  - larger files use the **content store** (IPFS CID, chunked to stay under the provider body limit) or the **iroh-blobs provider** (direct-P2P tickets, one per ciphertext chunk), with a content/publish fallback when the holder isn't reachable directly.

### Transport: the Elastos Runtime provider plane

hey-social never opens its own sockets — all delivery goes through runtime provider capabilities (declared in `capsule.json` and pre-warmed at boot in `src/main.rs`):

- **`elastos://peer/*`** — Carrier gossip (iroh 1.0-rc) for text / DM / group delivery. `peer_receiver` registers hey-social's federation handlers into the shared engine receiver and runs the one shared poll loop.
- **`elastos://content/*`** — content provider (IPFS / kubo) for media and large ciphertext, addressed by CID.
- **iroh-blobs provider** — direct-P2P large-file transfer via blob tickets.
- **`elastos://did/*`** — runtime identity.

### Auth: wallet-only / runtime-native SSO

There is **no in-capsule auth** — no passkey, no local seed. Identity comes only from the Elastos runtime: a launch token redeemed at boot (`runtime::redeem_launch_token`, scrubbed from the URL afterward), the identity provider (`identity/whoami`, a provider-backed did:key with no local seed), or an inherited runtime session (wallet SSO from Home). Without the runtime the app does not open (`pages/landing.rs`). `App` proactively clears any legacy session that still carries a local Ed25519 seed.

### UI shell

`lib.rs` wires a `leptos_router` `Router` whose base is derived from the iframe mount path (`/apps/hey-social/…`). App chrome — the sticky `TopHeader` (Hey wordmark, photo/video tabs, `ConnBadge`) and the `FloatingDock` — is rendered **once** at the App level so it persists across navigations, and is shown only on signed-in app routes. Modals (search, notifications, add-friend, contacts, following, new-group, link-phone) are mounted as siblings.

## Build

```
trunk build --release
```

`Trunk.toml` targets `index.html`, emits to `dist/`, and sets `public_url = "./"` so the capsule serves itself relative to its `/apps/hey-social/` mount. The committed `dist/` is what the runtime serves; remember to flatten `dist/* → .` for tarball-style deploys (the runtime serves capsule files from the root).

## Status

**Working and deployed** — login and posting are verified on the live runtime; feed, profile/follow, posts (with WebP compression), and PQ-E2E DMs + groups with attachments are functional. This is a shipping app, not a scaffold.

### Not implemented (do not assume these exist)

- No WebRTC voice / video / screen-share calls.
- No iroh-docs CRDT workspaces.
- No Teams-style workspaces / channels.
- No `hey-transcoder` provider in the pack — server-side transcoding is a pass-through, which is exactly why images are WebP-compressed in the browser and video uploads are size-capped.
