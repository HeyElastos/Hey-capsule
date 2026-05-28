# Hey Messenger Capsule

P2P messenger for the Elastos Runtime. Teams-shaped (workspaces, channels, calls, file share) but sovereign and serverless.

## Layers

| Concern | Transport |
| --- | --- |
| Identity | DID + Ed25519 signed events (extracted from Hey) |
| Text / presence / call signaling | Elastos Carrier gossip |
| File transfer | iroh-blobs direct peer-to-peer (no daemon staging, no nginx ceiling) |
| Workspace state (channels, pins, notes) | iroh-docs (CRDT) |
| Voice / video / screen share | WebRTC P2P, signaled over Carrier |

File sizes are bounded by local disk and receiver availability, not by HTTP body limits.

## Layout

```
hey-messenger-capsule/
├── capsule.json                   # app capsule manifest
├── Cargo.toml                     # rust workspace for providers
├── providers/
│   ├── blobs-provider/            # iroh-blobs direct transfer
│   ├── docs-provider/             # iroh-docs CRDT workspace
│   └── webrtc-signal-provider/    # WebRTC signaling over Carrier
└── client/                        # React UI
    └── src/lib/                   # messenger-core (signing, identity, carrier glue)
```

## Status

Phase 0 — scaffold only. See `docs/architecture.md` for the plan.
