# Architecture Audit — 2026-05

> Snapshot of the architectural decisions, dev framing, and concrete
> corrections that drove the work landed in commits `d0507f1` →
> `0f80f60` (2026-05-28). Read this alongside
> [runtime-quick-reference.md](runtime-quick-reference.md) to
> understand WHY the code looks the way it does today.

## The reset moment

The pack's app capsule used to be a React SPA (`capsules/hey-social/`)
that talked to the runtime through patches and assumptions that
diverged from the upstream contract. A dev review surfaced:

1. **Too much capsule-side identity work.** Hey ran its own passkey
   flow, derived its own `did:key:`, stored the Ed25519 seed and
   ML-KEM secret in `localStorage`. Any XSS in any bundled dep read
   the credentials.
2. **Bearer-token auth** instead of cookie auth. Hey extracted a
   bearer from `/runtime-token` and stamped `Authorization: Bearer …`
   on every fetch. Both the credential and the convention were
   capsule-managed.
3. **Direct IPFS access** (`ipfs.add_bytes`), assuming the capsule
   had ambient permission to talk to system providers.
4. **Reads from shared identity paths** (`.AppData/ElastOS/Identity/*`,
   `.AppData/Identity/*`) — outside the capsule's grant; the runtime
   was correct to reject them.
5. **Confused `principal` with social `did:key:`**. `person:local:…`
   showed up as the user's DID in the profile UI — wrong ontology.
6. **No federation actually happening.** The "peer provider" the chat
   code was calling didn't exist on the box; every `gossip_send` was
   silently dropped. Local writes made the UI look like it worked.

Dev framing in two sentences:

> The right fix is more providers, not more permissions. Strip the
> ambient access; build small focused providers; let the capsule
> ask each one for the surface it needs.

## What landed (capsule code)

### Hey-social cleanup (`d0507f1`, `0b8ac35`, `1b49573`, `be2d75a`)

- **`capsule.json` storage scope reduced to one entry:**
  `localhost://Users/self/.AppData/LocalHost/Hey/*`. Every other
  shared-identity / shell scope removed.
- **`capsule.json` messaging declares INTENT only** — `peer`, `content`,
  `identity`, `social-feed`, `did`, `hey-transcoder`, `elacity`. These
  are manifest-declared scopes the capsule may request at runtime;
  they are NOT automatic grants (per
  [runtime-quick-reference.md](runtime-quick-reference.md): no
  auto-grant policy ships today).
- **Shared-identity reads + dual-write deleted.**
  `.AppData/ElastOS/Identity/*` and `.AppData/Identity/*` paths are
  gone. `src/shell.rs` deleted. `ensure_profile` synthesizes from
  `session.did_key` only — the PRF-derived social DID.
- **PUT 412 (create-only conflict) downgraded to silent success.** The
  feed-index append pattern legitimately tries to overwrite a
  create-only file; treating that as a hard error spammed the UI.
- **First-run profile GET 404 is silent.** `storage::read_json`
  returns `Ok(None)` on 404, no log; `ensure_profile` writes the
  initial profile, future reads succeed.
- **Launch-token contract switched to cookie auth.**
  `redeem_launch_token` (renamed from `bearer_ready`) POSTs to
  `/api/apps/<id>/session/start` first, falls back to
  `/runtime-token` for older builds. Either response sets an HttpOnly
  cookie; the capsule no longer holds a bearer. Every
  `Authorization: Bearer …` injection site removed.
- **`inherit_session` DID-only filter.** Probe order: `didKey`,
  `did_key`, `did`, and nested `user./identity.` variants. `principal`
  intentionally excluded — even if a future principal happened to
  start with `did:`, it would still be the runtime principal, not the
  social DID.
- **Peer wire-shape compliance.** `peer_receiver` reads `content`
  field first, falls back to `message` (legacy). Per-pair queue
  topics + sealed-sender envelope kept (the spec explicitly allows
  this DM convention).

### New provider drafts (`c0353c7`, `959554d`, `0f80f60`)

#### [identity-projection-provider](../capsules/identity-projection-provider/)

Answers `elastos://identity/*` with `whoami / sign / verify`. Holds
the Ed25519 seed; capsules never see the secret. HKDF-derived
per-namespace keys for cross-capsule continuity.

**Status: draft.** `identity` is NOT in the runtime's
`RESERVED_SUB_NAMES` (see [STATUS.md](../capsules/identity-projection-provider/STATUS.md)
in the provider). To actually dispatch: patch the runtime registry,
rename to a non-reserved scheme, or use the YNH-fork patch path.

#### [content-provider](../capsules/content-provider/)

Answers `elastos://content/*` with `publish / fetch / ensure /
unpublish` on top of kubo. Maps policy hints ("network_default",
"local_pin", "transient") to pin lifecycle.

**Status: draft.** Upstream implements `elastos://content/*` as
`crate::content` (server-side, not a separate capsule). Installing
this on stock upstream is a no-op — the runtime short-circuits before
the provider registry is consulted. See
[STATUS.md](../capsules/content-provider/STATUS.md) for the three
options to actually wire dispatch.

### Build hygiene (`0b4a0db`)

[.github/workflows/verify-dist.yml](../.github/workflows/verify-dist.yml)
— on every push/PR rebuilds hey-social from a clean state and
compares the dist/ tree hash. The committed bundle must match a
clean rebuild from the same commit. Also builds every provider. The
WASM ↔ commit relationship is now reviewable.

## What was reverted

### peer-provider capsule (`76b7e58`, reverted in `1b49573`)

I built a standalone iroh-gossip capsule answering `elastos://peer/*`.
**It's redundant** — the runtime already provides this surface via
its built-in iroh stack. Capsules call `provider_call("peer", ...)`
and the runtime dispatches internally. The smoke test of my
peer-provider passed in isolation but the binary would never have
received a request on a real install.

Lesson: check `RESERVED_SUB_NAMES` (registry.rs:163) before building
a provider. Reserved schemes may already have built-in handlers.

## Open architectural decisions

These are unresolved as of 2026-05-28 and will need a call before
more code lands:

1. **What namespace does identity-projection-provider live under?**
   Three options in its STATUS.md. Picking one unblocks the
   capsule-to-provider migration.

2. **Does content-provider replace `crate::content` or sit beside it?**
   The dev framing wants a single content surface with transcode
   policy + dDRM. Server-side `crate::content` is a thin pass-through
   to kubo. Either we patch the runtime to delegate, or we accept
   the duplicate and let hey-social use whichever is dispatched.

3. **When do we file upstream PRs?**
   The YNH fork's patch 0001 adds hey-social/hey-messenger to the
   `/session/start` allowlist. A planned patch revision adds
   generic `/session/start` + OPTIONS support. Once stable, file
   upstream to make the allowlist configurable so we don't
   indefinitely fork.

4. **Hey-social keystore migration.**
   The Ed25519 seed + ML-KEM secret still live in localStorage. The
   identity-projection-provider exists with the right contract, but
   hey-social hasn't swapped its in-bundle derivation for
   `identity.sign` RPCs yet. Mechanical change once we resolve #1.

5. **DM routing scheme.**
   Hey-social uses per-pair random queue topics + sealed-sender
   envelopes. The dev's reference flow (chat-room) doesn't use the
   `\x01DM:` marker convention either, but uses runtime room-service
   objects, not gossip. Decide whether hey-social stays on
   gossip-with-queues or migrates to a future room-service-like
   surface.

## Pointers for the next agent

- [docs/runtime-quick-reference.md](runtime-quick-reference.md) — key
  truths first
- [docs/runtime-contract.md](runtime-contract.md) — full audit with
  source citations
- [`capsules/*/STATUS.md`](../capsules/) — per-provider status
- `git log --since=2026-05-25 --pretty=full` — commit messages carry
  the per-change reasoning
- [HeyElastos/elastos-runtime_ynh](https://github.com/HeyElastos/elastos-runtime_ynh)
  → `scripts/patches/` — the YNH-side patches against upstream
