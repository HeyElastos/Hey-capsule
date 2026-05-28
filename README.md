# Hey Capsule Pack

The HeyElastos capsule pack — every Hey-specific capsule, in one repo,
portable to any Elastos Runtime.

This repo is **YunoHost-agnostic**. The YunoHost package lives at
[HeyElastos/elastos-runtime_ynh](https://github.com/HeyElastos/elastos-runtime_ynh)
and fetches this pack at install time. Anything in here should work
unchanged against bare upstream Elastos Runtime, the YunoHost build,
or any future packaging.

## What's in the pack

| Capsule | Kind | What it does |
|---|---|---|
| [`capsules/hey-social/`](capsules/hey-social/) | app (React SPA) | Photo, video, and chat social app — feeds, carousels, reactions, DMs, group rooms. |
| [`capsules/hey-messenger/`](capsules/hey-messenger/) | app (React SPA) | P2P messenger with workspaces, calls, hybrid post-quantum E2E DMs, unlimited file share. |
| [`capsules/blobs-provider/`](capsules/blobs-provider/) | provider (Rust) | iroh-blobs direct peer-to-peer file transfer. Bypasses HTTP body limits. |
| [`capsules/docs-provider/`](capsules/docs-provider/) | provider (Rust, stub) | iroh-docs CRDT for shared workspace state. Phase 4. |
| [`capsules/webrtc-signal-provider/`](capsules/webrtc-signal-provider/) | provider (Rust, stub) | WebRTC SDP/ICE signaling over Carrier topics for the messenger's calls. Phase 5. |

## How a capsule pack is consumed

Each capsule subdirectory carries its own `capsule.json` (the Elastos
Runtime capsule manifest) plus either:
- a `client/` React project (for app capsules), built with `npm run build`,
- or a `Cargo.toml` (for provider capsules), built with `cargo build --release`.

A runtime that wants to install this pack fetches a tagged release
tarball, extracts `capsules/*`, runs the per-capsule build, and
registers each one with its `capsule.json`. That's it.

The pack does NOT ship pre-built bundles in the repo. Build artifacts
are produced either at install time by the consuming runtime, or by
this repo's CI workflow which can publish a `prebuilt-*` release for
fast installs.

## Modular contract with the runtime

The pack only talks to the runtime through stable HTTP contracts:

| Path | Purpose |
|---|---|
| `POST /api/provider/peer/*` | Carrier gossip (upstream-owned) |
| `POST /api/provider/ipfs/*` | IPFS (upstream-owned) |
| `POST /api/provider/blobs/*` | iroh-blobs (this pack provides) |
| `POST /api/provider/did/*` | DID resolve/sign/verify (upstream-owned) |
| `GET/PUT/DELETE /api/apps/:capsule/storage/*` | Principal-scoped storage (v0.3+) |
| `GET/PUT/DELETE /api/localhost/Users/self/*` | Sandboxed storage (upstream, v0.2 and any restored future) |
| `POST /api/apps/:capsule/runtime-token` | Launch envelope → session bearer |
| `POST /api/auth/passkey/*` | Passkey signup / sign-in (upstream-owned) |

The app capsules each ship a tiny **storage adapter** in `client/src/lib/runtime.js`
that probes the patch-0002 route and falls back to upstream-native
storage on 401/403/404. This is the only place runtime-API specifics
live, so future upstream changes touch one file per capsule.

When upstream needs surgical fixes for either auth or storage, those
patches live in elastos-runtime_ynh (the YunoHost package), not here.

## Build (all capsules)

```bash
# App capsules
( cd capsules/hey-social/client    && npm install && npm run build )
( cd capsules/hey-messenger/client && npm install && npm run build )

# Provider capsules (rustc 1.91+)
cargo build --release -p blobs-provider
cargo build --release -p docs-provider
cargo build --release -p webrtc-signal-provider
```

The Cargo workspace at the repo root covers all three Rust providers.

## License

MIT. See [LICENSE](LICENSE).
