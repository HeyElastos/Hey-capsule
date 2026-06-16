# Hey ⇄ BEAM native build (`mobile/beam/`)

Builds BEAM (Mimblewimble) wallet-core **from source** into `libbeam.so` for Android
(`arm64-v8a` + `x86_64`) plus Hey's **own thin JNI shim** (`hey_beam_jni.cpp`), and stages them into
the app's `jniLibs/` next to `libhey_mobile_runtime.so`.

Full design + verified API contract: [`../../docs/HEY_BEAM_INTEGRATION.md`](../../docs/HEY_BEAM_INTEGRATION.md).

> We do **not** vendor/copy BeamMW/android-wallet's JNI. We build BEAM core from a pinned tag and
> write our own minimal shim (`jni/`). No untrusted prebuilt blob ever handles funds.

## Why this dir exists (the blocker)

BEAM has **no Rust implementation**, so unlike the rest of Hey's pure-Rust money stack it needs a
C++ toolchain. The real cost is **static Boost + OpenSSL cross-compiled for both ABIs** — BEAM's own
`build_android.sh` hardcodes a dev's prebuilt paths and a single ABI, so it isn't reproducible.
`build-deps.sh` solves that; `build-beam.sh` then builds BEAM core + our shim against it.

## Prerequisites

- Android NDK (set `ANDROID_NDK_HOME`; r25c/r26 known-good — BEAM upstream used r21, newer works).
- `cmake` ≥ 3.22, `ninja`, `git`, `curl`, a host C++ toolchain, `perl` (OpenSSL Configure).
- ~6 GB free disk + time (Boost + OpenSSL × 2 ABIs + BEAM core is a long first build).
- `source ~/Android/env.sh` (same env the app's `build-apk.sh` uses) so `ANDROID_NDK_HOME` is set.

## Run order

```bash
export ANDROID_NDK_HOME=/path/to/ndk      # or: source ~/Android/env.sh
./build-deps.sh        # 1. OpenSSL + Boost static, per ABI  -> deps/<abi>/{openssl,boost}
./build-beam.sh        # 2. BEAM core (client lib) + our shim -> staged into the app jniLibs
```

Outputs land in `mobile/hey-social-app/app/src/main/jniLibs/{arm64-v8a,x86_64}/`:
`libbeam.so` (BEAM core + shim) and `libc++_shared.so`. Then the app's normal `build-apk.sh`
packages them.

## Pins

- `BEAM_TAG` (in `build-beam.sh`) — pin to a released tag, never HEAD. Re-verify the cited wallet-core
  symbols in `docs/HEY_BEAM_INTEGRATION.md` against the chosen tag's headers before trusting send.
- `BOOST_VERSION` (1.68 per BEAM) + `OPENSSL_VERSION` in `build-deps.sh`.

## Status

Phase 1 of the BEAM plan (see the design doc). The shim's C++ bodies that call BEAM wallet-core are
marked `// VERIFY:` — confirm each signature against the pinned tag's headers while iterating the
first local compile. **No BEAM send is enabled until the money-safety gate passes** (BIP39 vectors +
testnet public_offline tx + one sub-cent mainnet tx).
