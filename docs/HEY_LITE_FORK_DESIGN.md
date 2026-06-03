# hey-lite — no-KVM, container-native, cross-platform Hey (DRAFT)

> Design draft for a **new forked repo** (`HeyElastos/hey-lite`). This is a
> *packaging* fork of `elastos-runtime_ynh` + the `Hey-capsule` pack — **not** a
> third source of truth. Capsule source stays byte-identical to `Hey-capsule`
> so it remains portable to vanilla upstream.
>
> Status: design only. Grounded in a 4-agent audit of the real repos
> (2026-05-31). Two claims were adversarially re-verified (see callouts).

## 1. Goal

One `podman run` / `docker run` (or `compose up`) brings up the **whole Hey
experience** — Home auth + `hey-social` + `hey-chat` + P2P (`peer-provider`) +
the providers + the `/apps/` gateway — on **macOS, Windows, or Linux**, with:

- **No KVM / no microVM / no qemu.**
- **No domain, plain HTTP** at `http://localhost:PORT` (a browser secure context).
- **No YunoHost, no systemd** (container PID-1 entrypoint).

## 2. The two questions, answered (verified)

### Does removing `crosvm` + `vmlinux` + `qemu-system-x86`/`qemu-utils` break anything?
**No — it only drops the in-runtime *sandboxed browser* capsule (and microVM
chat). Hey itself is unaffected.**

- `crosvm`/`vmlinux` are **fetched external binaries** (`components.json`:
  `crosvm` = "VM monitor required for capsule microVM execution"; `vmlinux` =
  guest kernel). Excluding them from the profile means they're never downloaded.
- The rootfs builders (`scripts/build/build-vm-smoke-rootfs.sh`,
  `build-rootfs.sh`) only build images for `type:microvm` capsules — skipped.
- `qemu-*` apt deps are image tooling, not used by the core — dropped.
- `elastos serve` + `elastos room open` **do not spawn any microVM at startup**
  (`elastos-runtime-wrapper.sh:94-127`); the supervisor's VM path only fires on
  an explicit `launch_capsule`, and `main.rs:1907-1922` shows the runtime runs
  fine when `crosvm::is_supported()` is false.

> ⚠️ **Corrected during audit:** an agent flagged the Hey *providers*
> (`content`/`blobs`/`identity-projection`) as `type:microvm` → "need KVM."
> **That is wrong.** They are **registered as `bin/<name>` native binaries**
> (`components.additions.json`: `install_path: bin/blobs-provider`,
> `linux-amd64`/`arm64`) and run as **stdio host-process children** — the
> deployed runtime's `bin/` literally contains them as native executables. The
> `type:microvm` field in their `capsule.json` is **overridden** by the bin/
> registration. So they need **no KVM**. (One residual risk: confirm the runtime
> loads `content-provider` via the native stdio path in a no-crosvm image — §11.)

### Will it run on macOS / Windows / Linux via Docker/Podman?
**Yes. `needsKvmInContainer: false`.**

- The lite stack is Linux-native processes (`elastos serve` + stdio providers +
  kubo) + **WASM that runs in the user's own browser**, not in the container.
- mac/Windows run the Linux container inside Docker Desktop / podman-machine's
  Linux VM — **no nested KVM needed** because nothing spawns a microVM.
- **Apple Silicon**: build an arm64 image; it runs natively in the arm64 VM.

| Component | KVM? |
|---|---|
| Home, hey-social, hey-chat, peer-provider, all stdio providers, kubo, room gateway | ❌ none |
| Sandboxed **browser** capsule + microVM chat (EXCLUDED from hey-lite) | ✅ KVM |

## 3. What's kept vs lost

**Kept:** wallet/Home login, posting (photo/video), feed, follow/friend
requests, DMs (PQ-E2E), cross-runtime federation via iroh `peer-provider`,
IPFS media (kubo + `content`/`ipfs` provider), did/identity signing.

**Lost (microVM-only):** the in-runtime sandboxed *web browser* capsule, the
microVM full-screen *chat* variant. Also intentionally excluded upstream:
`browser`, `wallet`, `net-provider`, `exit-provider`, `chain-provider`
(same omissions the YNH install already makes).

## 4. Repo layout (`HeyElastos/hey-lite`)

```
/Containerfile            multi-stage (builder: rust 1.89+1.91+trunk; runtime: debian-slim + bins + wasm + kubo, NO qemu)
/entrypoint.sh            PID1 under tini; replaces wrapper + systemd.service
/UPSTREAM_VERSION         pin (v0.3.0), copied verbatim
/scripts/patches/0001..0006-*.patch   copied UNCHANGED; applied to fetched upstream at build
/scripts/components.additions.json    copied; merged onto upstream components.json
/scripts/build-lite.sh    source-build pipeline distilled from _common.sh (builder stage only)
/scripts/stage-home.sh    temp-publisher bootstrap + `elastos setup --with <LITE>` -> baked HOME
/conf/lite-shim.sh        plain-bash ynh_* shim lifted from install-bare.sh
/conf/home-overlay.css    frosted-glass theme (packaging concern)
/compose.yaml             named volume + 80:8090 + healthcheck
/README.md /LICENSE(MIT) /.dockerignore
# DROPPED: nginx.conf, systemd.service, sudoers, cli-wrapper, setup-crosvm.sh, manifest.toml, YNH lifecycle scripts
```

## 5. Containerfile (multi-stage sketch)

```dockerfile
# ── Stage 1: builder (fat, throwaway) ──
FROM debian:bookworm-slim AS builder
ARG UPSTREAM_VERSION=v0.3.0
ARG TARGETARCH                       # amd64|arm64 -> kubo + bin suffix
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git python3 openssl jq \
      build-essential pkg-config libssl-dev libclang-dev cmake \
 && rm -rf /var/lib/apt/lists/*        # NOTE: no qemu, no nodejs/npm
# rustup + BOTH channels: 1.89.0 (runtime + wasip1) AND 1.91 (iroh capsules)
ENV RUSTUP_HOME=/opt/rust/rustup CARGO_HOME=/opt/rust/cargo PATH=/opt/rust/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.89.0 --profile minimal \
 && rustup target add --toolchain 1.89.0 wasm32-wasip1 wasm32-unknown-unknown \
 && rustup toolchain install 1.91 --profile minimal \
 && rustup target add --toolchain 1.91 wasm32-wasip1 wasm32-unknown-unknown \
 && cargo install --locked trunk wasm-bindgen-cli
WORKDIR /build
COPY UPSTREAM_VERSION scripts/ conf/ ./
# Hey capsules: vendor a pinned snapshot OR curl the Hey-capsule tarball by sha256.
RUN bash scripts/build-lite.sh && bash scripts/stage-home.sh   # -> /out/home
RUN KARCH=${TARGETARCH:-amd64} \
 && curl -fsSL "https://dist.ipfs.tech/kubo/v0.40.1/kubo_v0.40.1_linux-${KARCH}.tar.gz" \
      | tar -xz -C /tmp && install -m0755 /tmp/kubo/ipfs /out/home/xdg-data/elastos/bin/kubo

# ── Stage 2: runtime (thin) ──
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl tini openssl && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --home-dir /home/elastos --shell /usr/sbin/nologin elastos
COPY --from=builder /entrypoint.sh /entrypoint.sh
# Ship baked HOME as a TEMPLATE at a non-volume path; entrypoint seeds the volume first-run.
COPY --from=builder --chown=elastos:elastos /out/home/ /opt/elastos/home-template/
ENV APP=elastos HOME=/var/lib/elastos/home XDG_DATA_HOME=/var/lib/elastos/home/xdg-data \
    IPFS_PATH=/var/lib/elastos/home/xdg-data/ipfs PORT=8090 ELASTOS_AUTH_GATE=1
USER elastos
EXPOSE 8090
VOLUME ["/var/lib/elastos/home"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=120s \
  CMD curl -fsS http://127.0.0.1:8090/apps/home/ >/dev/null || exit 1
ENTRYPOINT ["/usr/bin/tini","--","/entrypoint.sh"]
```

## 6. Entrypoint (no systemd, no nginx)

PID-1 under `tini`. Seeds the volume from the baked template on first run
(avoids the "VOLUME shadows baked HOME → blank runtime" trap), generates the
`.localhost-key` if absent, then: **kubo daemon → `elastos serve` (:3000) →
wait for coords+health → `elastos room open --addr 0.0.0.0:$PORT` → `wait` on
serve**. Providers (ipfs/content/blobs/peer/did/identity) are **stdio children
the runtime spawns on demand** — the entrypoint must NOT start them (same rule
the systemd wrapper documents). Full sketch in the audit transcript.

## 7. Lite components profile

`elastos setup --with <explicit list>` (NOT `--profile home`, which pulls
browser+wallet+net/exit/chain + the room-browser microVM):

```
shell, localhost-provider, did-provider, webspace-provider, ipfs-provider,
content-provider, blobs-provider, peer-provider, identity-projection-provider,
kubo, home, system, chat-room, inbox, library, documents, hey-social, hey-chat
```
- Native bins (cargo): `elastos-server`, shell, localhost (release); did/webspace/ipfs/content (release, 1.89); **blobs/peer/identity-projection (release, 1.91 via per-crate `rust-toolchain.toml`, `cd`-into-crate)**.
- WASM (wasip1): home-cli, home, system, chat-room.
- WASM apps (trunk): hey-social, hey-chat.
- External: kubo (arch-correct).
- **Excluded:** crosvm, vmlinux, room-browser microVM, qemu, setup-crosvm.sh, browser, wallet, net/exit/chain providers, webrtc-signal (stub).

## 8. Fork / strip plan

- **From `elastos-runtime_ynh`** (the source-build backbone): copy verbatim
  `UPSTREAM_VERSION`, `scripts/patches/0001-0006`, `components.additions.json`,
  `conf/home-overlay.css`. **Port** the de-ynh function bodies of
  `_common.sh` (`fetch_upstream_source`, `fetch_hey_capsules`,
  `build_runtime_and_capsules`, `stage_publisher_artifacts`, helpers) into
  `build-lite.sh` + `stage-home.sh`.
- **From `install-bare.sh`**: lift the `ynh_*` bash shim (it already proves the
  no-YunoHost, IP-only, HTTP path) + the temp-publisher bootstrap +
  `elastos setup --with` + `setup --profile operator` flow.
- **From `Hey`**: vendor a pinned snapshot (or sha256-fetch) of
  `capsules/{hey-social,hey-chat,hey-core,content-provider,blobs-provider,docs-provider,identity-projection-provider,peer-provider}` + workspace `Cargo.toml`, per-crate `rust-toolchain.toml`, `Trunk.toml`, prebuilt `dist/`. Keep byte-identical.
- **Strip** (container replaces): systemd.service, nginx.conf, sudoers,
  cli-wrapper, YNH lifecycle scripts, manifest.toml; the microVM toolchain
  (setup-crosvm.sh, qemu deps, crosvm/vmlinux fetch); nodejs/npm (the React
  build path is dead — apps are Rust/Leptos).

## 9. Patches 0001-0006 carry over UNCHANGED

Applied to the **fetched upstream at build time** (`patch -p1 --forward`, fatal
on failure), pin stays `v0.3.0` so context matches. They run **once in the
builder**; the runtime stage ships only compiled binaries (no patch at start).
0001 capsule runtime-token · 0002 principal storage · 0003 allowlist
hey-social/hey-chat through the provider proxy · 0004 reserve `identity` scheme ·
0005 register identity-projection-provider · 0006 inject principal into
`elastos://identity/*`. All open-access bridges with kill conditions; delete a
file the moment upstream merges the equivalent.

## 10. Networking, no-domain HTTP & secure context (cross-platform)

- Gateway binds `0.0.0.0:$PORT`; publish with `-p 80:8090`. Home at
  `http://<host>:80/apps/home/`. No nginx, no `__PATH__` subpath (mount at root).
- **`http://localhost:PORT` is a browser secure context** → WebCrypto/clipboard
  /auth work through the port-map on mac/win/linux. **But** a LAN IP
  (`http://192.168.x.x`) or custom hostname is **not** secure → needs TLS.
  (Safari has been inconsistent about localhost — smoke-test it.)
- **iroh P2P (`peer-provider`)** uses `presets::N0`: pkarr/DNS discovery + n0
  relay + QUIC, **ephemeral UDP port**. Federation works behind Docker NAT
  **outbound-only** (relay fallback) with **no inbound port map** — cost is
  latency + dependence on n0's public relays.
- **Linux only:** `--network host` removes the Docker NAT layer → more direct
  (hole-punched) connections. **mac/Windows: no host networking** (Docker
  Desktop) → `-p` only.
- **Independent mode** (fixed UDP port + public addr in ticket, no relay) is
  **impractical on Docker Desktop** (container can't self-discover the host's
  public IP; published UDP maps to the VM). Viable on a **public Linux host**
  (`--network host` + fixed port + env-injected public addr). Self-sovereign
  alternative: run your own iroh relay (outbound-only from clients).

## 11. Persistence — one volume

Mount **`/var/lib/elastos/home`** (= `$HOME`, with `xdg-data` + `ipfs` under
it). It captures **everything stateful**: `.local/bin/elastos`,
`xdg-data/elastos/{sources.json,capsules,runtime-coords.json,.localhost-key,
peer-provider/secret.key}`, the kubo repo (`$IPFS_PATH`), per-principal
storage. Losing it = new EndpointId (federation re-keys) + new IPFS peer id +
lost pins + unreadable encrypted store. **Prefer a named volume** over a host
bind-mount on Docker Desktop (perf + uid mapping).

## 12. Risks / open questions (must verify before shipping)

1. **content-provider native-load:** it's `type:microvm` in capsule.json but
   must load as a **native stdio** provider in a no-crosvm image (like
   ipfs-provider). It's also **missing from the current `_common.sh` build
   list** — `build-lite.sh` must add it AND confirm the stdio path. *(Highest-
   priority verify — it gates `elastos://content/*` media.)*
2. **First-run volume shadowing:** named volume copies image contents; a **bind
   mount shadows** the baked HOME → blank runtime. Mitigated by template-at-
   `/opt` + first-run copy; test BOTH named-volume and bind-mount.
3. **Dual toolchain:** `build-lite.sh` MUST `cd` into each iroh crate (1.91
   discovered by cwd-walk), not `--manifest-path`, or iroh crates silently build
   with 1.89 and fail on rc.1 APIs.
4. **Temp-publisher in a BUILD layer:** `stage-home.sh` spins `elastos serve` +
   loopback calls + teardown inside a RUN; BuildKit's process/network sandbox is
   fragile for the 60s waits + background reaping. May need a mini-init in the
   RUN, or move bootstrap to the entrypoint first-run path.
5. **Multi-arch build cost:** aws-lc-sys + iroh compile slowly/flaky under
   qemu-emulated buildx → prefer arch-native CI runners for arm64.
6. **PC2 divergence:** the *live* deployment dropped the gateway for PC2
   `/api/ipfs` + kvstore + app-level js-libp2p. hey-lite is the **gateway**
   model. Confirm the committed `hey-social`/`hey-chat` `dist/` targets gateway
   endpoints (not PC2) — else rebuild or `api_base` override.
7. **Image size + RAM:** thin runtime still ships elastos + ~8 provider bins +
   kubo + wasm + baked HOME; cold `cargo --release+lto` peaks ~4 GB (no in-image
   swap) → CI/host headroom.
8. **End-to-end only proven on Linux** (Jetson/YunoHost); mac/Windows Docker
   Desktop paths are reasoned-sound but **unverified** — smoke-test before
   claiming support.

## 13. Suggested build order
1. `build-lite.sh` + `lite-shim.sh` (port `_common.sh`, drop microVM/qemu/npm);
   verify a native Linux source build of the lite component set (esp.
   content-provider stdio).
2. `stage-home.sh` (temp-publisher → baked HOME) on the host first, then inside
   a RUN.
3. `entrypoint.sh` + Containerfile; `docker run -p 80:8090` on Linux → confirm
   Home login + post + same-host DM at `http://localhost`.
4. Two-container federation test (two containers, same host) → cross-runtime DM.
5. `buildx` arm64; smoke on Apple Silicon + Windows WSL2.
6. (Optional) wire the `peer-provider` **independent mode** (fixed port + full
   `EndpointAddr` ticket) for public-Linux zero-relay; doc the Docker-Desktop
   limitation.

## 14. Handoff — resuming in a fresh session

**Status: design only. Nothing built.** This doc is the spec; build from §13.

**Where things stand (2026-05-31):**
- The base transport this image ships — the iroh-gossip `peer-provider` + DM/friend
  cross-runtime federation — is **already built & pushed** in the source repos
  (`Hey` @ `be410d0`, `elastos-runtime_ynh` @ `cabe3be`). hey-lite is a *packaging*
  layer over that, not new app code.
- The current live runtimes use the **YunoHost** path; hey-lite is the container path.
- ⚠️ **PC2 divergence risk (verify first):** the most recent *live* deployment work
  pivoted hey-social/hey-chat toward PC2 endpoints, then was reverted on `main`.
  Confirm the committed `dist/` WASM you bake targets the **gateway** model
  (`/api/provider/*`, `/apps/`), not PC2 — else rebuild from source in the builder
  stage or pass an `api_base` override.

**Read these to build it:**
- This spec (§4 layout, §5 Containerfile, §6 entrypoint, §7 lite profile, §8 fork/strip, §12 risks, §13 order).
- `elastos-runtime_ynh/scripts/_common.sh` (port → `build-lite.sh`), `scripts/install-bare.sh`
  (lift the `ynh_*` shim + the no-YunoHost bootstrap), `scripts/patches/0001-0006` (copy verbatim),
  `conf/elastos-runtime-wrapper.sh` (→ `entrypoint.sh`).
- Upstream source extracted at `/tmp/v030-pristine/elastos-runtime-0.3.0` (pin = `UPSTREAM_VERSION` v0.3.0).

**First move:** §13 step 1 — a native Linux source build of the lite component set,
proving `content-provider` builds + loads as a **stdio** provider (it's `type:microvm`
in capsule.json but must run native; it's also missing from `_common.sh`'s build list).
That single check de-risks the whole fork.

**Decisions still open:** repo name (`hey-lite` suggested); whether to vendor a pinned
Hey-capsule snapshot vs sha256-fetch the tarball; bake-time vs first-run temp-publisher
bootstrap; whether to ship prebuilt `dist/` or rebuild WASM in the builder.

(Memory: see `project_hey_lite_fork.md` for the condensed handoff and cross-refs.)
