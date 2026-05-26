# Hey Social — Architecture Summary

**Status**: Active development
**Last updated**: 2026-05-26
**Scope**: Photo, video, and chat social app on Elastos. Capsule-native,
federated over Elastos Carrier, media on IPFS, sovereign identity via
Ed25519 + did:key.

---

## What Hey Social is

A capsule-native social app. There is no backend server, no central
database, no email/password. Each user is a node on the Elastos Carrier
mesh; their identity is an Ed25519 keypair encoded as a did:key; their
posts and messages are signed events gossiped peer-to-peer; their media
is content-addressed in IPFS.

Two parallel deployments exist:

- **Capsule mode** (production target). Hey Social runs as a WASM
  capsule inside the Elastos Runtime. The React app talks to the
  runtime's HTTP surface: `/api/localhost/*` for storage, `/api/provider/*`
  for Carrier / IPFS / DID / transcoder. No Hey-owned backend.
- **Server mode** (legacy, being phased out). The original Express
  backend on port 4000 plus a JSON store. Currently staged for deletion
  in the working tree; the capsule mode covers all features end-to-end.

The build target is selected at build time via `VITE_HEY_MODE`.

---

## Stack

### Client (`client/`)

Vite + React 18 + Tailwind. The same React tree serves both deployment
shapes; the difference is in the API layer.

**Pages** ([client/src/pages/](client/src/pages/))
- `Landing.jsx` — unauthenticated entry, brand mark
- `SignUp.jsx` / `SignIn.jsx` — recovery key + optional passkey
- `Onboarding.jsx` — profile setup
- `Home.jsx` — photo feed
- `Clips.jsx` — video feed
- `Profile.jsx` — own + others' profiles
- `Chat.jsx` — DMs and rooms

**Lib** ([client/src/lib/](client/src/lib/))
- `runtime.js` — runtime HTTP client (`storage`, `peer`, `ipfs`, `did`,
  `transcoder`, `capability`), capability-token gated
- `identity.js` — Ed25519 + did:key derivation (`@noble/curves`)
- `keystore.js` — IndexedDB-backed non-extractable Web Crypto signing key
- `session.js` — boots the IDB key into a sync cache; async setters
- `events.js` — signed envelope construction (`createSignedEvent`,
  `verifySignedEvent`) using the non-extractable key
- `shell.js` — shared-identity bridge between Hey and hey-home
- `mode.js` — capsule vs. server mode detection

**API** ([client/src/api/](client/src/api/)) — every public export branches
on `isCapsuleMode()`. Capsule branch uses only runtime APIs; server branch
uses axios against the legacy Express backend.

### Capsule deployment (`hey-capsule` repo)

Lives outside this repo at `hey-capsule/`. Packages Hey Social as an
`elastos.capsule/v1` microvm: rootfs.ext4 staged from Hey's built bundle
plus a Node runtime and busybox. The capsule declares `ipfs-provider` as
a required capsule and requests `localhost://` + `elastos://peer/hey-v0/*`
permissions.

---

## How data moves

| Data | Where | Why |
|---|---|---|
| Profile, follows, notifications, message cache | `/api/localhost/.AppData/LocalHost/Hey/*` (runtime storage) | Local-first; persists across reinstalls; the runtime owns the file |
| Identity (DID, recoveryKeyHash, passkeys) | `/api/localhost/.AppData/Identity/profile.json` (shared with hey-home) | Same identity across Hey and the desktop shell |
| Posts, comments, reactions, follow events | Carrier gossip (`hey-v0/user/<did>/posts`, `hey-v0/follow/<did>`) | Federated, signed envelopes |
| DMs, group chats, voice messages, reactions | Carrier gossip on per-thread topics | Same envelope shape as posts |
| Photos, videos, voice clips, avatars | IPFS via `ipfs-provider` (Kubo) + Hey transcoder for normalization | Content-addressed, durable, dedup |
| Signing key | Non-extractable Ed25519 CryptoKey in IndexedDB | XSS cannot exfiltrate |

---

## Security posture

### Identity & signing

- **Recovery key**: 32 random bytes generated client-side at signup.
  Shown once, never persisted in storage. Only its SHA-256 hash
  (`recoveryKeyHash`) lives on disk.
- **Ed25519 keypair**: derived from the recovery key via `@noble/curves`
  at signup/sign-in, then imported as a NON-EXTRACTABLE Web Crypto
  CryptoKey and persisted as a CryptoKey handle in IndexedDB. The raw
  seed is zeroed in JS memory immediately after import.
- **Passkeys** (WebAuthn / FIDO2): optional second factor. Credential
  IDs + public keys stored in the profile; the actual private key lives
  in the OS / hardware authenticator.
- **DID format**: `did:key:z…` (W3C CCG spec; Ed25519 multicodec
  prefix + base58btc).

### Event integrity

Every federated event is a signed envelope:
`{ type, payload, sender_did, ts, signature }`. Signatures are over a
canonical (sorted-keys) JSON serialization so wire round-trips don't
break verification. Inbound events are signature-verified before being
trusted.

### Transport

- **CSP** in `client/index.html` — `script-src 'self'`, `connect-src 'self'`,
  `object-src 'none'`, etc. Defense in depth against XSS-injected
  `<script>` tags.
- **All runtime calls are same-origin** (`/api/*` on the capsule gateway).
  No external endpoints.
- **Capability tokens** — every runtime call sends an
  `X-Capability-Token`. Tokens are acquired at boot via
  `/api/capability/request`; the runtime can deny or require user
  approval.

### Known surfaces

- XSS while a tab is open can still call `crypto.subtle.sign()` on the
  non-extractable key — but cannot steal it.
- Profile JSON on disk is not encrypted at rest; relies on runtime/host
  filesystem permissions.
- DM payloads are signed but NOT encrypted end-to-end (any peer
  subscribed to the gossip topic could read them). Encryption layer is
  a planned addition.

---

## Internet posture

Hey Social is internet-agnostic by design. Carrier (Iroh — QUIC + DHT +
relay) discovers peers over mDNS on LAN and over the DHT across the
internet. IPFS (Kubo) does the same. Two devices on the same WiFi can
federate even when the WiFi has no upstream connection.

No outbound calls to Hey-owned servers. No analytics. No ads.

---

## Running

### Capsule mode (production target)

Built and packaged by the `hey-capsule` repo. Install on an Elastos
Runtime that has `ipfs-provider` available; launch from the shell
(hey-home or stock home).

### Server mode (legacy)

```bash
# Backend
cd server && npm install && npm start         # listens on :4000
# Frontend
cd client && npm install && npm run dev       # listens on :3000
```

Open <http://localhost:3000>, pick a nickname, copy the generated
recovery key, you're in.

---

## See also

- [README.md](README.md) — user-facing setup and feature overview
- [LICENSE](LICENSE) — MIT
- `hey-capsule/` (separate repo) — the capsule packaging for Elastos Runtime
- `elastos-runtime-ynh/capsules/hey-home/` — the desktop shell capsule
  that hosts Hey Social
