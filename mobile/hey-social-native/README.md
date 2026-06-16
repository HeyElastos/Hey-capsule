# Hey Social — native Android (on-device runtime)

A self-contained Android app: the **Elastos mini-runtime + iroh carrier run
inside the app process**, not on a remote server. The user just opens *Hey
Social*; there is no "connect to a host", no token paste, no wallet.

This is the native counterpart to `mobile/hey-shell` (the thin remote-window
client). Here the phone **is** the runtime.

## Architecture

```
APK (os.elastos.hey.social)
├─ WebView  →  http://127.0.0.1:8787/apps/hey-social/   (unmodified Leptos/WASM UI)
├─ lib/arm64-v8a/libhey_mobile_runtime.so               (the whole runtime, one .so)
│    ├─ loopback HTTP server   — answers /api/provider/*, storage, content, session
│    ├─ carrier                — iroh 1.0-rc.1 + iroh-gossip (elastos://peer/*)
│    ├─ identity               — local seed + ML-KEM (elastos://identity/*)
│    ├─ storage                — per-capsule files
│    └─ content                — local content-addressed store (+ /ipfs gateway)
├─ assets/dist/                — the prebuilt hey-social WASM bundle (staged to filesDir)
└─ RuntimeService              — foreground service, keeps the carrier meshed in background
```

The runtime crate is `capsules/hey-mobile-runtime` (shared by hey-social and,
later, hey-chat). It reuses `hey-core` verbatim, so on-device crypto/identity is
byte-identical to the browser and CLI builds.

## Build

```bash
./build-apk.sh           # cross-compiles the .so, stages it + dist, runs gradle
# -> app/build/outputs/apk/debug/app-debug.apk
```

Prereqs: `~/Android/env.sh` (JAVA_HOME / ANDROID_HOME / NDK_HOME), the
`aarch64-linux-android` Rust target on the **1.91** toolchain (iroh requires it),
and `cargo-ndk`.

## Run

```bash
source ~/Android/env.sh
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n os.elastos.hey.social/.MainActivity
adb logcat -s HeyRuntime:I   # watch the runtime + carrier come up
```

On-host emulators are unreliable here (qemu/GL vs the host kernel) — run on a
real arm64 phone.

## Status

- **Proven on host:** the runtime serves the real hey-social UI and answers
  identity / carrier-ticket / sign / storage / content over loopback.
- **Built:** arm64 `.so` (NDK r26) + APK package verified to contain the .so and
  the WASM dist.
- **Pending on-device verification:** runtime boot inside the APK on a physical
  phone, and a cross-device DM (the iroh carrier behind mobile NAT/CGNAT — relay
  fallback expected on symmetric NAT).

## Identity / auth (next layer)

v1 holds the identity (`seed + ML-KEM`) in `filesDir/hey/identity.json`
(plaintext; the OS already sandboxes per-app). The hardware path is wired but
not yet enabled: `HeyRuntime.nativeStart(..., identityBlob)` accepts a
Keystore-unlocked blob, so the next step is a `BiometricPrompt` + Android
Keystore/StrongBox-wrapped vault that decrypts the blob and passes it in — the
"fingerprint to open Hey" flow. `androidx.biometric` is already a dependency.
